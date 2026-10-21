// Exercises the actual central workflow code with local files and a fake API.
// No network calls, tokens, macOS builds or GitHub writes are involved.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const workflow = fs.readFileSync(path.join(root, 'ci/turner/turner-build.yml'), 'utf8');
const sourceWorkflow = fs.readFileSync(path.join(root, '.github/workflows/build-compact-macos.yml'), 'utf8');
const buildScript = fs.readFileSync(path.join(root, 'scripts/build-compact-macos.command'), 'utf8');
const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor;
function actualStep(marker) {
  const expression = new RegExp('^\\s*// BEGIN ' + marker + '\\n([\\s\\S]*?)^\\s*// END ' + marker + '$', 'm');
  const matched = workflow.match(expression);
  assert.ok(matched, 'the deployed template must expose its actual ' + marker + ' step');
  return new AsyncFunction('github', 'context', 'core', 'require', 'process', matched[1].replace(/^ {12}/gm, ''));
}
const publish = actualStep('TURNER_RELEASE_PUBLISHER');
const prepare = actualStep('TURNER_RELEASE_REQUEST');
const require = createRequire(import.meta.url);
const version = fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8').match(/^version = "([0-9.]+)"$/m)[1];
const revision = Number(fs.readFileSync(path.join(root, 'egui/src/version.rs'), 'utf8').match(/^pub const COMPACT_REVISION: u64 = ([1-9][0-9]*);$/m)[1]);
const tag = 'turner-v' + version + '-r' + revision;
const zipName = 'Turner-v' + version + '-r' + revision + '-macos-x86_64.app.zip';
const checksumName = zipName + '.sha256';
const manifestName = 'turner-update.json';
const sourceCommit = 'a'.repeat(40);
const centralCommit = 'b'.repeat(40);
const feedBlob = 'c'.repeat(40);
const beforeCommit = 'd'.repeat(40);
const feedCommit = 'e'.repeat(40);
const sha256 = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');
const missing = () => Object.assign(new Error('not found'), { status: 404 });

function contextFor(options) {
  return {
    eventName: options.event ?? 'workflow_dispatch',
    repo: { owner: 'Tangram-Space', repo: 'updates' },
    ref: options.ref ?? 'refs/heads/main',
    sha: centralCommit,
    payload: {
      inputs: { publish: options.publish ?? 'true' },
      repository: { default_branch: 'main' },
      before: beforeCommit,
    },
  };
}

function fixture(t, options = {}) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'turner-release-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  // ZIP structure and Mach-O are checked by the macOS job before this code.
  const zip = Buffer.from('fixture representing the verified Intel ZIP');
  const substitutions = {
    version, COMPACT_REVISION: String(revision), RELEASE_TAG: tag,
    MINIMUM_MACOS: '11.0', BUNDLE_IDENTIFIER: 'com.tuner.guitar.compact',
    zip_name: zipName, UPDATE_REPOSITORY: 'Tangram-Space/updates', sha256: sha256(zip),
  };
  const template = buildScript.match(/cat > "\$stage\/\$manifest_name" <<EOF\n([\s\S]*?)\nEOF/)[1];
  const manifest = JSON.parse(template.replace(/\$([a-zA-Z_][a-zA-Z_0-9]*)/g, (_, key) => {
    assert.ok(key in substitutions, 'unexpected manifest field ' + key);
    return substitutions[key];
  }));
  Object.assign(manifest, options.manifest);
  const manifestBytes = Buffer.from(JSON.stringify(manifest));
  const checksumBytes = Buffer.from(sha256(zip) + '  ' + zipName + '\n');
  fs.writeFileSync(path.join(directory, zipName), zip);
  fs.writeFileSync(path.join(directory, checksumName), checksumBytes);
  fs.writeFileSync(path.join(directory, manifestName), manifestBytes);
  const writes = [], uploaded = [], notices = [], warnings = [];
  const existingFiles = [
    { name: zipName, bytes: zip }, { name: checksumName, bytes: checksumBytes },
    { name: manifestName, bytes: manifestBytes },
  ];
  if (options.resume) {
    for (const file of existingFiles) uploaded.push({
      name: file.name, state: 'uploaded', size: file.bytes.length,
      digest: 'sha256:' + sha256(file.bytes),
    });
  }
  let branchReads = 0;
  const repos = {
    getBranch: async () => ({
      data: { commit: { sha: ++branchReads > 1 && options.lateBranchChange ? 'f'.repeat(40) : options.branchSha ?? centralCommit } },
    }),
    getContent: async ({ path: filename, ref }) => {
      assert.equal(filename, 'feeds/turner/stable/release.json');
      assert.equal(ref, centralCommit);
      if (options.noFeed) throw missing();
      return { data: {
        type: 'file', encoding: 'base64', sha: options.actualFeedSha ?? feedBlob,
        content: Buffer.from(JSON.stringify({
          ...manifest, revision: options.feedRevision ?? revision - 1,
        })).toString('base64'),
      } };
    },
    listTags() {}, listReleases() {}, listReleaseAssets() {},
    createRelease: async (request) => {
      writes.push({ operation: 'draft', request });
      return { data: { id: 42, draft: true, tag_name: tag } };
    },
    uploadReleaseAsset: async (request) => {
      writes.push({ operation: 'upload', name: request.name });
      if (options.uploadFailure === request.name) throw new Error('simulated upload failure');
      uploaded.push({
        name: request.name, state: 'uploaded', size: request.data.length,
        digest: 'sha256:' + sha256(request.data),
      });
      return { data: uploaded.at(-1) };
    },
    updateRelease: async (request) => {
      writes.push({ operation: 'publish', request });
      return { data: stableRelease() };
    },
  };
  function stableRelease() {
    return {
      id: 42, tag_name: tag, draft: false, prerelease: false,
      html_url: 'https://github.com/Tangram-Space/updates/releases/tag/' + tag,
    };
  }
  const github = {
    rest: { repos, git: {
      getRef: async () => ({ data: { object: { type: 'commit', sha: options.existingTagCommit ?? centralCommit } } }),
      getCommit: async ({ commit_sha }) => {
        assert.equal(commit_sha, centralCommit);
        return { data: { tree: { sha: '1'.repeat(40) } } };
      },
      createRef: async (request) => {
        if (options.tagRace) throw new Error('tag was concurrently created');
        writes.push({ operation: 'tag', request });
        return { data: { ref: request.ref, object: { sha: request.sha } } };
      },
      createBlob: async (request) => {
        writes.push({ operation: 'feed-blob', request });
        return { data: { sha: '2'.repeat(40) } };
      },
      createTree: async (request) => {
        writes.push({ operation: 'feed-tree', request });
        return { data: { sha: '3'.repeat(40) } };
      },
      createCommit: async (request) => {
        writes.push({ operation: 'feed-commit', request });
        return { data: { sha: feedCommit } };
      },
      updateRef: async (request) => {
        if (options.feedRefRace) throw new Error('non-fast-forward: another application advanced main');
        writes.push({ operation: 'feed-ref', request });
        return { data: { object: { sha: request.sha } } };
      },
    } },
    paginate: async (endpoint) => {
      if (endpoint === repos.listTags) return (options.tags ?? []).map((name) => ({ name }));
      if (endpoint === repos.listReleases) {
        return [
          ...(options.resume ? [stableRelease()] : []),
          ...(options.releases ?? []).map((item) => typeof item === 'string' ? { ...stableRelease(), tag_name: item } : item),
        ];
      }
      assert.equal(endpoint, repos.listReleaseAssets, 'unexpected API call');
      const result = structuredClone(uploaded);
      if (options.missingUpload) result.pop();
      if (options.wrongUploadHash) result[0].digest = 'sha256:' + '0'.repeat(64);
      if (options.missingUploadDigest) delete result[0].digest;
      return result;
    },
  };
  const core = { setOutput() {}, notice: (message) => notices.push(message), warning: (message) => warnings.push(message) };
  const env = {
    TURNER_ARTIFACT_DIRECTORY: directory, EXPECTED_SOURCE_REPO: 'Chevosky/tuner-compact',
    EXPECTED_SOURCE_SHA: sourceCommit, EXPECTED_VERSION: version, EXPECTED_REVISION: String(revision),
    EXPECTED_TAG: tag, EXPECTED_FEED_SHA: options.noFeed ? '' : feedBlob,
  };
  return { directory, writes, notices, warnings, manifestBytes, run: () => publish(github, contextFor(options), core, require, { env }) };
}

test('publishes verified assets without taking global latest, then commits the byte-identical Turner feed', async (t) => {
  const subject = fixture(t);
  await subject.run();
  assert.deepEqual(subject.writes.map((entry) => entry.operation), [
    'tag', 'draft', 'upload', 'upload', 'upload', 'publish',
    'feed-blob', 'feed-tree', 'feed-commit', 'feed-ref',
  ]);
  assert.equal(subject.writes[0].request.sha, centralCommit);
  assert.equal(subject.writes[0].request.ref, 'refs/tags/' + tag);
  const draft = subject.writes[1].request;
  assert.equal(draft.draft, true);
  assert.equal(draft.make_latest, 'false');
  assert.match(draft.body, new RegExp(sourceCommit));
  assert.deepEqual(subject.writes.slice(2, 5).map((entry) => entry.name), [zipName, checksumName, manifestName]);
  assert.equal(subject.writes[5].request.make_latest, 'false');
  assert.equal(subject.writes[5].request.draft, false);
  assert.deepEqual(Buffer.from(subject.writes[6].request.content, 'base64'), subject.manifestBytes);
  assert.deepEqual(subject.writes[7].request.tree, [
    { path: 'feeds/turner/stable/release.json', mode: '100644', type: 'blob', sha: '2'.repeat(40) },
  ]);
  assert.equal(subject.writes[7].request.base_tree, '1'.repeat(40));
  assert.deepEqual(subject.writes[8].request.parents, [centralCommit]);
  assert.deepEqual(subject.writes[9].request, {
    owner: 'Tangram-Space', repo: 'updates', ref: 'heads/main', sha: feedCommit, force: false,
  });
  assert.equal(subject.notices.length, 1);
});

test('a new channel can create its first feed without touching other applications', async (t) => {
  const subject = fixture(t, { noFeed: true, tags: ['airplay-v999.0.0-b99999999'], event: 'push' });
  await subject.run();
  assert.equal(subject.writes.at(-1).operation, 'feed-ref');
});

for (const [label, options] of [
  ['a normal build', { publish: 'false' }],
  ['an unrelated event', { event: 'pull_request' }],
  ['a non-default branch', { ref: 'refs/heads/experimental' }],
  ['a stale central commit', { branchSha: 'f'.repeat(40) }],
  ['a changed feed lease', { actualFeedSha: 'f'.repeat(40) }],
  ['a feed rollback', { feedRevision: revision }],
  ['a mismatched build revision', { manifest: { revision: revision + 1 } }],
  ['the wrong app channel', { manifest: { app_id: 'airplay' } }],
  ['an external download URL', { manifest: { url: 'https://example.invalid/update.zip' } }],
  ['a reserved Git tag', { tags: [tag] }],
  ['a draft release', { releases: [{ id: 42, tag_name: tag, draft: true }] }],
  ['a concurrent tag creation', { tagRace: true }],
  ['a lower global Turner revision', { tags: ['turner-v' + version + '-r' + (revision + 1)] }],
  ['a lower app version', { tags: ['turner-v' + (BigInt(version.split('.')[0]) + 1n) + '.0.0-r' + Math.max(1, revision - 1)] }],
]) {
  test(label + ' cannot change any remote object', async (t) => {
    const subject = fixture(t, options);
    await assert.rejects(subject.run());
    assert.deepEqual(subject.writes, []);
  });
}

for (const name of [zipName, checksumName, manifestName]) {
  test('missing ' + name + ' cannot publish', async (t) => {
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
  ['an absent remote digest', { missingUploadDigest: true }],
]) {
  test(label + ' leaves the release unpublished and feed untouched', async (t) => {
    const subject = fixture(t, options);
    await assert.rejects(subject.run());
    assert.equal(subject.writes[1].operation, 'draft');
    assert.ok(!subject.writes.some((entry) => ['publish', 'feed-blob', 'feed-ref'].includes(entry.operation)));
    assert.equal(subject.notices.length, 0);
    assert.equal(subject.warnings.length, 1);
  });
}

test('another app advancing main after upload prevents a stale feed write', async (t) => {
  const subject = fixture(t, { lateBranchChange: true });
  await assert.rejects(subject.run(), /central default branch changed/);
  assert.equal(subject.writes.at(-1).operation, 'publish');
});

test('a last-moment branch race cannot overwrite another application commit', async (t) => {
  const subject = fixture(t, { feedRefRace: true });
  await assert.rejects(subject.run(), /non-fast-forward/);
  assert.ok(!subject.writes.some((entry) => entry.operation === 'feed-ref'));
  assert.deepEqual(subject.writes.at(-1).request.parents, [centralCommit]);
});

test('identical published assets can resume only the feed step without editing the release', async (t) => {
  const subject = fixture(t, { resume: true });
  await subject.run();
  assert.deepEqual(subject.writes.map((entry) => entry.operation), ['feed-blob', 'feed-tree', 'feed-commit', 'feed-ref']);
});

for (const [label, options] of [
  ['a different source request commit', { existingTagCommit: 'f'.repeat(40) }],
  ['a changed release asset', { wrongUploadHash: true }],
]) {
  test(label + ' cannot resume the stable feed', async (t) => {
    const subject = fixture(t, { resume: true, ...options });
    await assert.rejects(subject.run());
    assert.deepEqual(subject.writes, []);
  });
}

function requestFixture(options = {}) {
  const outputs = {};
  const request = {
    schema_version: 1, app_id: 'turner', channel: 'stable',
    source_repo: 'Chevosky/tuner-compact', source_sha: sourceCommit, version, revision,
    ...options.request,
  };
  const github = { rest: { repos: {
    getBranch: async () => ({ data: { commit: { sha: options.branchSha ?? centralCommit } } }),
    getCommit: async () => {
      if (options.missingCommit) throw missing();
      return { data: { sha: beforeCommit } };
    },
    getContent: async ({ path: filename, ref }) => {
      let bytes, sha = '7'.repeat(40);
      if (filename === 'apps/turner/release.json') {
        let data = request;
        if (ref === beforeCommit) {
          if (options.missingPreviousFile) throw missing();
          data = { ...request, revision: options.beforeRevision ?? revision - 1 };
        }
        bytes = Buffer.from(JSON.stringify(data));
      } else if (filename === 'feeds/turner/stable/release.json') {
        if (options.noFeed) throw missing();
        sha = feedBlob;
        bytes = Buffer.from(JSON.stringify({
          schema_version: 1, app_id: 'turner', channel: 'stable', revision: options.feedRevision ?? revision - 1,
        }));
      } else {
        assert.equal(filename, '.github/workflows/turner-build.yml');
        bytes = Buffer.from(workflow);
      }
      return { data: { type: 'file', encoding: 'base64', sha, content: bytes.toString('base64') } };
    },
  } } };
  return { outputs, run: () => prepare(github, contextFor(options), { setOutput: (key, value) => { outputs[key] = value; } }, require, { env: {} }) };
}

test('request pins the public source SHA and exact deployed workflow, capturing its old feed lease', async () => {
  const subject = requestFixture();
  await subject.run();
  assert.deepEqual(subject.outputs, {
    source_repo: 'Chevosky/tuner-compact', source_sha: sourceCommit, version,
    revision: String(revision), tag, feed_sha: feedBlob,
    workflow_sha256: sha256(workflow), publish: 'true',
  });
});

test('manual run defaults to building only and does not require a newer release', async () => {
  const subject = requestFixture({ publish: 'false', feedRevision: revision });
  await subject.run();
  assert.equal(subject.outputs.publish, 'false');
});

test('first explicit request push starts a new channel', async () => {
  const subject = requestFixture({ event: 'push', missingPreviousFile: true, noFeed: true });
  await subject.run();
  assert.equal(subject.outputs.feed_sha, '');
  assert.equal(subject.outputs.publish, 'true');
});

for (const [label, options] of [
  ['an unpinned source ref', { request: { source_sha: 'compact-intel' } }],
  ['another source repository', { request: { source_repo: 'elsewhere/untrusted' } }],
  ['unknown request fields', { request: { extra: true } }],
  ['a noninteger revision', { request: { revision: 4.5 } }],
  ['a non-default branch', { ref: 'refs/heads/feature' }],
  ['a stale request commit', { branchSha: 'f'.repeat(40) }],
  ['an unchanged request push', { event: 'push', beforeRevision: revision }],
  ['a lower request revision', { event: 'push', beforeRevision: revision + 1 }],
  ['an unavailable previous push commit', { event: 'push', missingCommit: true }],
  ['a nonadvancing stable revision', { feedRevision: revision }],
]) {
  test(label + ' cannot authorize publication', async () => {
    const subject = requestFixture(options);
    await assert.rejects(subject.run());
    assert.deepEqual(subject.outputs, {});
  });
}

test('workflows gate publication on tests and exact source; source builds never publish', () => {
  const buildJob = workflow.split('  macos-intel:\n')[1].split('  publish-release:\n')[0];
  const publicationJob = workflow.split('  publish-release:\n')[1];
  assert.match(workflow, /branches: \[main\]\n\s+paths: \[apps\/turner\/release\.json\]/);
  assert.match(workflow, /publish:\n(?:[^\n]*\n){1,4}\s+default: false\n\s+type: boolean/);
  assert.match(buildJob, /ref: \$\{\{ needs\.release-request\.outputs\.source_sha \}\}/);
  assert.match(buildJob, /EXPECTED_WORKFLOW_SHA256/);
  assert.match(buildJob, /node scripts\/test-compact-release\.mjs/);
  assert.match(buildJob, /cargo test --locked -p audio-input -p guitar-tuner-egui/);
  assert.ok(buildJob.indexOf('cargo test --locked') < buildJob.indexOf('bash scripts/build-compact-macos.command'));
  assert.match(buildJob, /Turner\.app\/Contents\/MacOS/);
  assert.match(buildJob, /updates::tests::real_release_fixture_when_explicitly_provided -- --exact/);
  assert.ok(buildJob.indexOf('bash scripts/build-compact-macos.command') < buildJob.indexOf('TUNER_QA_RELEASE_ZIP:'));
  assert.ok(buildJob.indexOf('TUNER_QA_RELEASE_ZIP:') < buildJob.indexOf('actions/upload-artifact'));
  assert.doesNotMatch(buildJob, /continue-on-error|if:.*always\(\)/);
  assert.match(publicationJob, /needs: \[release-request, macos-intel\]/);
  assert.match(publicationJob, /needs\.macos-intel\.result == 'success'/);
  assert.doesNotMatch(publicationJob, /actions\/checkout|cargo (build|test)/);
  assert.equal(workflow.match(/contents: write/g).length, 1);
  assert.match(workflow, /permissions:\n  contents: read/);
  assert.match(publicationJob, /cancel-in-progress: false/);
  assert.doesNotMatch(sourceWorkflow, /contents: write|publish-release:|^\s+push:/m);
  assert.match(sourceWorkflow, /workflow_dispatch:/);
  assert.match(buildScript, /readonly APP_NAME="Turner"/);
  assert.match(buildScript, /readonly UPDATE_REPOSITORY="Tangram-Space\/updates"/);
  assert.match(buildScript, /readonly MINIMUM_MACOS="11\.0"/);
});
