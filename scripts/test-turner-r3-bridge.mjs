// Executes the real one-time legacy publisher with local files and a fake API.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const workflow = fs.readFileSync(path.join(root, '.github/workflows/turner-r3-bridge.yml'), 'utf8');
const source = workflow.match(/^\s*\/\/ BEGIN TURNER_BRIDGE_PUBLISHER\n([\s\S]*?)^\s*\/\/ END TURNER_BRIDGE_PUBLISHER$/m);
assert.ok(source, 'test the exact deployed publisher');
const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor;
const publish = new AsyncFunction('github', 'context', 'core', 'require', 'process', source[1].replace(/^ {12}/gm, ''));
const require = createRequire(import.meta.url);
const version = '0.1.13', revision = 4;
const tag = 'compact-v' + version + '-r' + revision;
const canonicalTag = 'turner-v' + version + '-r' + revision;
const zipName = 'Tuner-Compact-v' + version + '-r' + revision + '-macos-x86_64.app.zip';
const checksumName = zipName + '.sha256', manifestName = 'tuner-compact-update.json';
const sourceCommit = 'a'.repeat(40), canonicalHash = 'b'.repeat(64);
const digest = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');

function fixture(t, options = {}) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'turner-bridge-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const zip = Buffer.from('root-only repack fixture; byte preservation is tested in Python');
  const manifest = {
    schema_version: 1, version, revision, tag, platform: 'macos', arch: 'x86_64', minimum_macos: '11.0',
    bundle_id: 'com.tuner.guitar.compact', executable: 'guitar-tuner-egui', asset: zipName,
    url: 'https://github.com/Chevosky/tuner-compact/releases/download/' + tag + '/' + zipName,
    sha256: digest(zip), ...options.manifest,
  };
  const manifestBytes = Buffer.from(JSON.stringify(manifest));
  fs.writeFileSync(path.join(directory, zipName), zip);
  fs.writeFileSync(path.join(directory, checksumName), digest(zip) + '  ' + zipName + '\n');
  fs.writeFileSync(path.join(directory, manifestName), manifestBytes);
  const writes = [], uploaded = [], notices = [], warnings = [];
  const repos = {
    getBranch: async () => ({ data: { commit: { sha: options.branchSha ?? sourceCommit } } }),
    getContent: async ({ path: filename, ref }) => {
      assert.equal(filename, 'ci/turner/r3-bridge.json');
      assert.equal(ref, sourceCommit);
      const marker = {
        schema_version: 1, app_id: 'turner', from_revision: 3,
        canonical_tag: canonicalTag, canonical_manifest_sha256: canonicalHash, ...options.marker,
      };
      return { data: { type: 'file', encoding: 'base64', content: Buffer.from(JSON.stringify(marker)).toString('base64') } };
    },
    listTags() {}, listReleases() {}, listReleaseAssets() {},
    createRelease: async (request) => {
      writes.push({ operation: 'draft', request });
      return { data: { id: 42, draft: true, tag_name: tag } };
    },
    uploadReleaseAsset: async (request) => {
      writes.push({ operation: 'upload', name: request.name });
      if (options.uploadFailure === request.name) throw new Error('simulated upload failure');
      uploaded.push({ name: request.name, state: 'uploaded', size: request.data.length, digest: 'sha256:' + digest(request.data) });
    },
    updateRelease: async (request) => {
      writes.push({ operation: 'publish', request });
      return { data: { tag_name: tag, draft: false, prerelease: false, html_url: 'https://github.com/Chevosky/tuner-compact/releases/tag/' + tag } };
    },
  };
  const github = {
    rest: { repos, git: {
      createRef: async (request) => {
        if (options.tagRace) throw new Error('concurrent tag reservation');
        writes.push({ operation: 'tag', request });
        return { data: { ref: request.ref, object: { sha: request.sha } } };
      },
    } },
    paginate: async (endpoint) => {
      if (endpoint === repos.listTags) return (options.tags ?? ['compact-v0.1.13-r3']).map((name) => ({ name }));
      if (endpoint === repos.listReleases) return (options.releases ?? []).map((tag_name) => ({ tag_name }));
      assert.equal(endpoint, repos.listReleaseAssets);
      const result = structuredClone(uploaded);
      if (options.missingUpload) result.pop();
      if (options.wrongDigest) result[0].digest = 'sha256:' + '0'.repeat(64);
      return result;
    },
  };
  const context = {
    eventName: options.event ?? 'workflow_dispatch',
    repo: { owner: 'Chevosky', repo: 'tuner-compact' }, sha: sourceCommit,
    ref: options.ref ?? 'refs/heads/compact-intel',
    payload: { repository: { default_branch: 'compact-intel' }, inputs: { publish: options.publish ?? 'true' } },
  };
  const env = {
    BRIDGE_ARTIFACT_DIRECTORY: directory, EXPECTED_VERSION: version, EXPECTED_REVISION: String(revision),
    EXPECTED_TAG: tag, EXPECTED_CANONICAL_TAG: canonicalTag,
    EXPECTED_CANONICAL_MANIFEST_SHA256: canonicalHash, EXPECTED_LEGACY_MANIFEST_SHA256: digest(manifestBytes),
  };
  const core = { notice: (message) => notices.push(message), warning: (message) => warnings.push(message) };
  return { directory, writes, notices, warnings, run: () => publish(github, context, core, require, { env }) };
}

test('publishes the verified r3 bridge only after all three assets are complete', async (t) => {
  const subject = fixture(t, { event: 'push' });
  await subject.run();
  assert.deepEqual(subject.writes.map((entry) => entry.operation), ['tag', 'draft', 'upload', 'upload', 'upload', 'publish']);
  assert.equal(subject.writes[0].request.sha, sourceCommit);
  assert.equal(subject.writes[1].request.draft, true);
  assert.equal(subject.writes[1].request.make_latest, 'false');
  assert.match(subject.writes[1].request.body, /conserva la ruta Tuner Compact\.app/);
  assert.deepEqual(subject.writes.slice(2, 5).map((entry) => entry.name), [zipName, checksumName, manifestName]);
  assert.equal(subject.writes[5].request.make_latest, 'true');
  assert.equal(subject.writes[5].request.owner, 'Chevosky');
  assert.equal(subject.writes[5].request.repo, 'tuner-compact');
  assert.equal(subject.notices.length, 1);
});

for (const [label, options] of [
  ['build-only manual run', { publish: 'false' }],
  ['unrelated event', { event: 'pull_request' }],
  ['wrong branch', { ref: 'refs/heads/main' }],
  ['stale source commit', { branchSha: 'c'.repeat(40) }],
  ['changed canonical pin', { marker: { canonical_manifest_sha256: 'c'.repeat(64) } }],
  ['wrong canonical tag', { marker: { canonical_tag: 'turner-v0.1.13-r5' } }],
  ['external ZIP URL', { manifest: { url: 'https://example.invalid/app.zip' } }],
  ['nonadvancing revision', { manifest: { revision: 3 } }],
  ['reused legacy tag', { tags: [tag] }],
  ['previously published bridge', { releases: [tag] }],
  ['another reserved post-r3 revision', { tags: ['compact-v0.1.13-r5'] }],
  ['version rollback', { tags: ['compact-v0.2.0-r3'] }],
  ['concurrent tag reservation', { tagRace: true }],
]) {
  test(label + ' cannot create a bridge tag or release', async (t) => {
    const subject = fixture(t, options);
    await assert.rejects(subject.run());
    assert.deepEqual(subject.writes, []);
  });
}

for (const filename of [zipName, checksumName, manifestName]) {
  test('missing ' + filename + ' cannot publish', async (t) => {
    const subject = fixture(t);
    fs.unlinkSync(path.join(subject.directory, filename));
    await assert.rejects(subject.run());
    assert.deepEqual(subject.writes, []);
  });
}

test('manifest and ZIP tampering fail before writing remotely', async (t) => {
  for (const filename of [manifestName, zipName]) {
    const subject = fixture(t);
    fs.appendFileSync(path.join(subject.directory, filename), 'changed');
    await assert.rejects(subject.run());
    assert.deepEqual(subject.writes, []);
  }
});

test('an unexpected file or symlink cannot become a release asset', async (t) => {
  const extra = fixture(t);
  fs.writeFileSync(path.join(extra.directory, 'extra'), 'extra');
  await assert.rejects(extra.run(), /exactly its three/);
  assert.deepEqual(extra.writes, []);
  const link = fixture(t);
  fs.unlinkSync(path.join(link.directory, checksumName));
  fs.symlinkSync(path.join(link.directory, zipName), path.join(link.directory, checksumName));
  await assert.rejects(link.run(), /Invalid bridge asset/);
  assert.deepEqual(link.writes, []);
});

for (const [label, options] of [
  ['failed upload', { uploadFailure: checksumName }],
  ['missing uploaded asset', { missingUpload: true }],
  ['wrong remote digest', { wrongDigest: true }],
]) {
  test(label + ' leaves the bridge in draft', async (t) => {
    const subject = fixture(t, options);
    await assert.rejects(subject.run());
    assert.ok(!subject.writes.some((entry) => entry.operation === 'publish'));
    assert.equal(subject.warnings.length, 1);
  });
}

test('the bridge is an explicit marker workflow, with no Rust build or cross-repository writes', () => {
  assert.match(workflow, /branches: \[compact-intel\]\n\s+paths: \[ci\/turner\/r3-bridge\.json\]/);
  assert.match(workflow, /default: false\n\s+type: boolean/);
  assert.doesNotMatch(workflow, /cargo |rustup |macos-15|secrets\./);
  assert.match(workflow, /python3 scripts\/test-turner-r3-bridge\.py/);
  assert.match(workflow, /node scripts\/test-turner-r3-bridge\.mjs/);
  assert.equal(workflow.match(/contents: write/g).length, 1);
  const publisher = workflow.split('  publish-bridge:\n')[1];
  assert.match(publisher, /needs: verified-repack/);
  assert.match(publisher, /needs\.verified-repack\.result == 'success'/);
  assert.doesNotMatch(publisher, /actions\/checkout/);
  assert.match(publisher, /cancel-in-progress: false/);
});
