// Runs the actual inline publisher with local files and a fake GitHub API.
// No network calls, tokens, macOS build or release creation are involved.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const workflow = fs.readFileSync(path.join(root, '.github/workflows/build-compact-macos.yml'), 'utf8');
const buildScript = fs.readFileSync(path.join(root, 'scripts/build-compact-macos.command'), 'utf8');
const publisherSource = workflow.match(/^\s*\/\/ BEGIN COMPACT_RELEASE_PUBLISHER\n([\s\S]*?)^\s*\/\/ END COMPACT_RELEASE_PUBLISHER$/m);
assert.ok(publisherSource, 'the workflow must expose its real publisher for isolated tests');
const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor;
const publish = new AsyncFunction('github', 'context', 'core', 'require', 'process', publisherSource[1].replace(/^ {12}/gm, ''));
const require = createRequire(import.meta.url);
const version = fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8').match(/^version = "([0-9.]+)"$/m)[1];
const revision = Number(fs.readFileSync(path.join(root, 'egui/src/version.rs'), 'utf8').match(/^pub const COMPACT_REVISION: u64 = ([1-9][0-9]*);$/m)[1]);
const tag = `compact-v${version}-r${revision}`;
const zipName = `Tuner-Compact-v${version}-r${revision}-macos-x86_64.app.zip`;
const checksumName = `${zipName}.sha256`;
const manifestName = 'tuner-compact-update.json';
const sourceCommit = 'a'.repeat(40);
const beforeCommit = 'b'.repeat(40);
const sha256 = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');

function fixture(t, options = {}) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'tuner-release-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  // Archive structure and Mach-O validation run in the preceding macOS build.
  // Here the important property is hashing exactly the bytes being uploaded.
  const zip = Buffer.from('fixture representing the verified Intel ZIP');
  const substitutions = {
    version,
    COMPACT_REVISION: String(revision),
    RELEASE_TAG: tag,
    MINIMUM_MACOS: '11.0',
    BUNDLE_IDENTIFIER: 'com.tuner.guitar.compact',
    zip_name: zipName,
    UPDATE_REPOSITORY: 'Chevosky/tuner-compact',
    sha256: sha256(zip),
  };
  const template = buildScript.match(/cat > "\$stage\/\$manifest_name" <<EOF\n([\s\S]*?)\nEOF/)[1];
  const manifest = JSON.parse(template.replace(/\$([a-zA-Z_][a-zA-Z_0-9]*)/g, (_, key) => {
    assert.ok(key in substitutions, `unexpected build manifest field ${key}`);
    return substitutions[key];
  }));
  Object.assign(manifest, options.manifest);
  fs.writeFileSync(path.join(directory, zipName), zip);
  fs.writeFileSync(path.join(directory, checksumName), `${sha256(zip)}  ${zipName}\n`);
  fs.writeFileSync(path.join(directory, manifestName), JSON.stringify(manifest));
  const writes = [];
  const uploaded = [];
  const notices = [];
  const warnings = [];
  const context = {
    eventName: options.event ?? 'workflow_dispatch',
    repo: { owner: 'Chevosky', repo: 'tuner-compact' },
    ref: options.ref ?? 'refs/heads/compact-intel',
    sha: sourceCommit,
    payload: {
      inputs: { publish: options.publish ?? 'true' },
      repository: { default_branch: 'compact-intel' },
      before: beforeCommit,
    },
  };
  const missing = () => Object.assign(new Error('not found'), { status: 404 });
  const repos = {
    getBranch: async () => ({ data: { commit: { sha: options.branchSha ?? sourceCommit } } }),
    getCommit: async () => {
      if (options.missingCommit) throw missing();
      return { data: { sha: beforeCommit } };
    },
    getContent: async () => {
      if (options.missingPreviousFile) throw missing();
      const previous = options.beforeRevision ?? revision - 1;
      return { data: {
        type: 'file', encoding: 'base64',
        content: Buffer.from(`pub const COMPACT_REVISION: u64 = ${previous};\n`).toString('base64'),
      } };
    },
    listTags() {},
    listReleases() {},
    listReleaseAssets() {},
    createRelease: async (request) => {
      writes.push({ operation: 'draft', request });
      return { data: { id: 42, draft: true, tag_name: tag } };
    },
    uploadReleaseAsset: async (request) => {
      writes.push({ operation: 'upload', name: request.name });
      if (options.uploadFailure === request.name) throw new Error('simulated upload failure');
      uploaded.push({ name: request.name, state: 'uploaded', size: request.data.length, digest: `sha256:${sha256(request.data)}` });
      return { data: uploaded.at(-1) };
    },
    updateRelease: async (request) => {
      writes.push({ operation: 'publish', request });
      return { data: { tag_name: tag, draft: false, prerelease: false, html_url: `https://github.com/Chevosky/tuner-compact/releases/tag/${tag}` } };
    },
  };
  const github = {
    rest: {
      repos,
      git: {
        createRef: async (request) => {
          if (options.tagRace) throw new Error('tag was concurrently created');
          writes.push({ operation: 'tag', request });
          return { data: { ref: request.ref, object: { sha: request.sha } } };
        },
      },
    },
    paginate: async (endpoint) => {
      if (endpoint === repos.listTags) return (options.tags ?? []).map((name) => ({ name }));
      if (endpoint === repos.listReleases) return (options.releases ?? []).map((tag_name) => ({ tag_name }));
      assert.equal(endpoint, repos.listReleaseAssets, 'unexpected GitHub API call');
      const result = structuredClone(uploaded);
      if (options.missingUpload) result.pop();
      if (options.wrongUploadHash) result[0].digest = `sha256:${'0'.repeat(64)}`;
      return result;
    },
  };
  const core = { setOutput() {}, notice: (message) => notices.push(message), warning: (message) => warnings.push(message) };
  const env = {
    COMPACT_ARTIFACT_DIRECTORY: directory,
    EXPECTED_VERSION: version,
    EXPECTED_REVISION: String(revision),
    EXPECTED_TAG: tag,
  };
  return {
    directory, writes, notices, warnings,
    run: () => publish(github, context, core, require, { env }),
  };
}

test('explicit manual publication uploads every verified file before making a release latest', async (t) => {
  const subject = fixture(t);
  await subject.run();
  assert.deepEqual(subject.writes.map((entry) => entry.operation), ['tag', 'draft', 'upload', 'upload', 'upload', 'publish']);
  assert.equal(subject.writes[0].request.sha, sourceCommit);
  assert.equal(subject.writes[0].request.ref, `refs/tags/${tag}`);
  assert.equal(subject.writes[1].request.draft, true);
  assert.equal(subject.writes[1].request.prerelease, false);
  assert.equal(subject.writes[1].request.target_commitish, sourceCommit);
  assert.deepEqual(subject.writes.slice(2, 5).map((entry) => entry.name), [zipName, checksumName, manifestName]);
  assert.equal(subject.writes.at(-1).request.make_latest, 'true');
  assert.equal(subject.writes.at(-1).request.draft, false);
  assert.equal(subject.notices.length, 1);
});

for (const [label, options] of [
  ['a normal build', { publish: 'false' }],
  ['an unrelated event', { event: 'pull_request' }],
  ['a non-default branch', { ref: 'refs/heads/experimental' }],
  ['a stale build commit', { branchSha: 'c'.repeat(40) }],
  ['a mismatched build revision', { manifest: { revision: revision + 1 } }],
  ['an external download URL', { manifest: { url: 'https://example.invalid/update.zip' } }],
  ['a reused release tag', { releases: [tag] }],
  ['a reused Git tag', { tags: [tag] }],
  ['a concurrent tag creation', { tagRace: true }],
  ['a lower global revision', { tags: [`compact-v${version}-r${revision + 1}`] }],
  ['a lower application version', { tags: [`compact-v${BigInt(version.split('.')[0]) + 1n}.0.0-r${Math.max(1, revision - 1)}`] }],
]) {
  test(`${label} cannot write a tag or release`, async (t) => {
    const subject = fixture(t, options);
    await assert.rejects(subject.run());
    assert.deepEqual(subject.writes, []);
  });
}

for (const name of [zipName, checksumName, manifestName]) {
  test(`missing ${name} cannot publish`, async (t) => {
    const subject = fixture(t);
    fs.unlinkSync(path.join(subject.directory, name));
    await assert.rejects(subject.run());
    assert.deepEqual(subject.writes, []);
  });
}

test('modified ZIP bytes fail before any remote write', async (t) => {
  const subject = fixture(t);
  fs.appendFileSync(path.join(subject.directory, zipName), 'corruption');
  await assert.rejects(subject.run(), /checksum do not agree/);
  assert.deepEqual(subject.writes, []);
});

test('unexpected files and symbolic links are rejected', async (t) => {
  const extra = fixture(t);
  fs.writeFileSync(path.join(extra.directory, 'unexpected.txt'), 'extra');
  await assert.rejects(extra.run(), /exactly the ZIP/);
  assert.deepEqual(extra.writes, []);
  const symlink = fixture(t);
  fs.unlinkSync(path.join(symlink.directory, checksumName));
  fs.symlinkSync(path.join(symlink.directory, zipName), path.join(symlink.directory, checksumName));
  await assert.rejects(symlink.run(), /Invalid release asset/);
  assert.deepEqual(symlink.writes, []);
});

for (const [label, options] of [
  ['an upload failure', { uploadFailure: checksumName }],
  ['a missing uploaded file', { missingUpload: true }],
  ['an incorrect remote asset hash', { wrongUploadHash: true }],
]) {
  test(`${label} leaves the release unpublished`, async (t) => {
    const subject = fixture(t, options);
    await assert.rejects(subject.run());
    assert.equal(subject.writes[1].operation, 'draft');
    assert.ok(!subject.writes.some((entry) => entry.operation === 'publish'));
    assert.equal(subject.notices.length, 0);
    assert.equal(subject.warnings.length, 1);
  });
}

test('a revision bump pushed to the default branch can publish after the build gate', async (t) => {
  const subject = fixture(t, { event: 'push' });
  await subject.run();
  assert.equal(subject.writes.at(-1).operation, 'publish');
});

test('the initial addition of version.rs is a publication signal', async (t) => {
  const subject = fixture(t, { event: 'push', missingPreviousFile: true });
  await subject.run();
  assert.equal(subject.writes.at(-1).operation, 'publish');
});

for (const [label, options] of [
  ['an unchanged revision', { beforeRevision: revision }],
  ['a revision rollback', { beforeRevision: revision + 1 }],
  ['an unavailable previous commit', { missingCommit: true }],
]) {
  test(`${label} cannot publish on push`, async (t) => {
    const subject = fixture(t, { event: 'push', ...options });
    await assert.rejects(subject.run());
    assert.deepEqual(subject.writes, []);
  });
}

test('workflow gates publication on successful tests/build and defaults manual runs to build-only', () => {
  const buildJob = workflow.split('  macos-intel:\n')[1].split('  publish-release:\n')[0];
  const publicationJob = workflow.split('  publish-release:\n')[1];
  assert.match(workflow, /branches: \[compact-intel\]\n\s+paths: \[egui\/src\/version\.rs\]/);
  assert.match(workflow, /publish:\n(?:[^\n]*\n){1,4}\s+default: false\n\s+type: boolean/);
  assert.match(buildJob, /node scripts\/test-compact-release\.mjs/);
  assert.match(buildJob, /cargo test --locked -p audio-input -p guitar-tuner-egui/);
  assert.ok(buildJob.indexOf('cargo test --locked') < buildJob.indexOf('bash scripts/build-compact-macos.command'));
  assert.match(buildJob, /TUNER_QA_RELEASE_ZIP:/);
  assert.match(buildJob, /updates::tests::real_release_fixture_when_explicitly_provided -- --exact/);
  assert.ok(buildJob.indexOf('bash scripts/build-compact-macos.command') < buildJob.indexOf('TUNER_QA_RELEASE_ZIP:'));
  assert.ok(buildJob.indexOf('TUNER_QA_RELEASE_ZIP:') < buildJob.indexOf('actions/upload-artifact'));
  assert.doesNotMatch(buildJob, /continue-on-error|if:.*always\(\)/);
  assert.match(publicationJob, /needs: macos-intel/);
  assert.match(publicationJob, /needs\.macos-intel\.result == 'success'/);
  assert.match(publicationJob, /inputs\.publish == true/);
  assert.doesNotMatch(publicationJob, /actions\/checkout|cargo (build|test)/);
  assert.equal(workflow.match(/contents: write/g).length, 1);
  assert.match(workflow, /permissions:\n  contents: read/);
  assert.match(publicationJob, /cancel-in-progress: false/);
});
