const {test} = require('node:test');
const assert = require('node:assert/strict');
const {readFileSync} = require('node:fs');
const path = require('node:path');
const {mkdtempSync, mkdirSync, writeFileSync, chmodSync, cpSync, symlinkSync, rmSync, existsSync} = require('node:fs');
const {tmpdir} = require('node:os');
const {spawnSync} = require('node:child_process');
const root=path.resolve(__dirname,'..');
const workflow=readFileSync(path.join(root,'.github/workflows/release.yml'),'utf8');
const ci=readFileSync(path.join(root,'.github/workflows/ci.yml'),'utf8');
function job(name) {return workflow.split(`\n  ${name}:\n`)[1].split(/\n  [a-z][\w-]*:\n/)[0];}
test('caller pins the shared implementation and keeps its repository-scoped bot credentials',()=>{
  const release=job('release');
  const [,sha]=release.match(/uses: tilr-dev\/\.github\/\.github\/workflows\/release-branch.yml@([a-f0-9]{40})\n/);
  assert.equal(sha, 'c31b80fcfd5cb9398a193d422c212326302d0c44');
  assert.ok(release.includes(`workflow-sha: ${sha}\n`));
  assert.doesNotMatch(ci, /shared-release|npm test/);
  assert.match(release,/contents: read/);
  assert.match(release,/app-client-id: \$\{\{ vars.TILR_BOT_CLIENT_ID \}\}/);
  assert.match(release,/app-private-key: \$\{\{ secrets.TILR_BOT_PRIVATE_KEY \}\}/);
  assert.match(workflow,/on:\n  push:\n    branches: \['release\/\*\*'\]/);
  assert.match(workflow,/tags: \['v\*', '!v\*-rc\.\*'\]/);
  assert.match(workflow,/cancel-in-progress: false\n  queue: max/);
  assert.match(job('build'),/if: needs.release.outputs.tag != ''/);
  assert.match(job('build'),/ref: \$\{\{ needs.release.outputs.tag \}\}/);
  assert.match(job('upload'),/gh release upload/);
});
test('candidates build with their tag version and bypass only crates.io publication',()=>{
  assert.doesNotMatch(job('release'), /package-path|release_created/);
  assert.doesNotMatch(workflow, /release-please|release_created/);
  assert.match(job('publish'),/if: needs.release.outputs.tag != '' && needs.release.outputs.is_rc == 'false' && startsWith\(github.ref, 'refs\/tags\/v'\)/);
  assert.match(job('publish'),/environment: crates-io/);
  assert.match(job('publish'),/id-token: write/);
  assert.match(job('build'),/GITPM_BUILD_VERSION="\$\{TAG#v\}" cargo build/);
  assert.match(job('upload'),/!cancelled\(\) && needs.build.result == 'success'/);
  assert.match(job('upload'),/needs.publish.result == 'success' \|\| needs.release.outputs.is_rc == 'true'/);
});

test('Apple assets use the tag and gate stable and candidate uploads',()=>{
  const apple=job('build-apple');
  assert.match(apple,/if: needs.release.outputs.tag != ''/);
  assert.match(apple,/runs-on: macos-15/);
  assert.match(apple,/ref: \$\{\{ needs.release.outputs.tag \}\}/);
  assert.match(apple,/contents: read/);
  assert.doesNotMatch(apple,/contents: write|id-token: write/);
  assert.match(apple,/--target aarch64-apple-darwin,x86_64-apple-darwin/);
  assert.match(apple,/scripts\/build-apple.sh --release/);
  assert.match(apple,/name: gitpm-ffi-universal-apple-darwin/);
  assert.match(apple,/path: dist\/\*/);
  assert.match(apple,/if-no-files-found: error/);
  assert.match(job('upload'),/needs: \[release, publish, build, build-apple\]/);
  assert.match(job('upload'),/needs.build-apple.result == 'success'/);
  assert.match(job('upload'),/pattern: gitpm-\*/);
});

test('every release checkout prepares tag versions before Cargo; dirty publish is intentional',()=>{
  for (const name of ['publish','build','build-apple']) {
    const content=job(name);
    assert.match(content, /needs: release/);
    assert.match(content, /RELEASE_TAG: \$\{\{ needs.release.outputs.tag \}\}/);
    assert.match(content, /python3 scripts\/set-release-version.py "\$RELEASE_TAG"/);
    assert.ok(content.indexOf('set-release-version.py') < content.indexOf('rustup toolchain'));
  }
  assert.match(job('publish'), /cargo publish -p "\$crate" --locked --allow-dirty/);
  assert.ok(job('publish').indexOf('cargo test') < job('publish').indexOf('id: auth'));
});

test('release-please configuration is removed',()=>{
  for (const file of ['release-please-config.json','.release-please-manifest.json']) {
    assert.equal(existsSync(path.join(root,file)), false);
  }
});

// Exercise the real helper and Cargo resolver on tiny isolated workspaces. External
// dependencies in the repository lockfile must remain byte-for-byte unchanged too.
for (const tag of ['v2.3.4','v2.3.4-rc.12']) {
  test(`Cargo accepts prepared versions and lockfile for ${tag}`,()=>{
    const dir=mkdtempSync(path.join(tmpdir(),'gitpm-version-'));
    try {
      writeFileSync(path.join(dir,'Cargo.toml'), '[workspace]\nmembers = ["crates/*"]\nresolver = "2"\n');
      const names=['gitpm','gitpm-cli','gitpm-ffi'];
      for (const name of names) {
        const crate=path.join(dir,'crates',name);
        mkdirSync(path.join(crate,'src'),{recursive:true});
        writeFileSync(path.join(crate,'Cargo.toml'), `[package]\nname = "${name}"\nversion = "0.5.0"\nedition = "2021"\n`+
          (name==='gitpm' ? '' : '[dependencies]\ngitpm = { path = "../gitpm", version = "0.5.0" }\n'));
        writeFileSync(path.join(crate,'src/lib.rs'), 'pub const VERSION: &str = env!("CARGO_PKG_VERSION");\n');
      }
      const cargo=args=>spawnSync('cargo',args,{cwd:dir,encoding:'utf8'});
      const initial=cargo(['generate-lockfile','--offline']);
      assert.equal(initial.status,0,initial.stderr);
      const run=()=>spawnSync('python3',[path.join(root,'scripts/set-release-version.py'),tag],{cwd:dir,encoding:'utf8'});
      const prepared=run(); assert.equal(prepared.status,0,prepared.stderr);
      const lock=readFileSync(path.join(dir,'Cargo.lock'),'utf8');
      const metadata=cargo(['metadata','--locked','--offline','--format-version','1']);
      assert.equal(metadata.status,0,metadata.stderr);
      for (const pkg of JSON.parse(metadata.stdout).packages) {
        assert.equal(pkg.version,tag.slice(1));
        for (const dep of pkg.dependencies) assert.equal(dep.req,`^${tag.slice(1)}`);
      }
      assert.equal(readFileSync(path.join(dir,'Cargo.lock'),'utf8'),lock);
      assert.equal(run().status,0);
      assert.equal(readFileSync(path.join(dir,'Cargo.lock'),'utf8'),lock);
      // Real manifest/lockfile shape, including all registry entries.
      for (const name of names) cpSync(path.join(root,'crates',name,'Cargo.toml'),path.join(dir,'crates',name,'Cargo.toml'));
      cpSync(path.join(root,'Cargo.lock'),path.join(dir,'Cargo.lock'));
      const actual=run(); assert.equal(actual.status,0,actual.stderr);
      const normalized=readFileSync(path.join(dir,'Cargo.lock'),'utf8').replaceAll(`version = "${tag.slice(1)}"`,`version = "0.5.0"`);
      assert.equal(normalized,readFileSync(path.join(root,'Cargo.lock'),'utf8'));
    } finally {rmSync(dir,{recursive:true,force:true});}
  });
}

test('invalid tags and malformed metadata fail before writing files',()=>{
  const dir=mkdtempSync(path.join(tmpdir(),'gitpm-version-invalid-'));
  try {
    for (const name of ['gitpm','gitpm-cli','gitpm-ffi']) {
      const crate=path.join(dir,'crates',name);
      mkdirSync(crate,{recursive:true});
      cpSync(path.join(root,'crates',name,'Cargo.toml'),path.join(crate,'Cargo.toml'));
    }
    const manifest=path.join(dir,'crates/gitpm/Cargo.toml');
    const before=readFileSync(manifest,'utf8');
    for (const tag of ['2.3.4','v02.3.4','v2.3.4-rc.0','v2.3.4-beta.1','v2.3.4\n']) {
      const result=spawnSync('python3',[path.join(root,'scripts/set-release-version.py'),tag],{cwd:dir,encoding:'utf8'});
      assert.notEqual(result.status,0); assert.match(result.stderr,/expected vX.Y.Z/);
    }
    writeFileSync(path.join(dir,'Cargo.lock'),'# missing workspace records\n');
    const result=spawnSync('python3',[path.join(root,'scripts/set-release-version.py'),'v2.3.4'],{cwd:dir,encoding:'utf8'});
    assert.notEqual(result.status,0); assert.match(result.stderr,/expected one match/);
    assert.equal(readFileSync(manifest,'utf8'),before);
  } finally {rmSync(dir,{recursive:true,force:true});}
});

// These tools replace compilation/Xcode/GitHub; the real scripts, tar and hash
// utilities execute. The isolated PATH also makes missing xcodebuild deterministic.
const stub = `#!${process.execPath}
const fs = require('node:fs');
const path = require('node:path');
const args = process.argv.slice(2);
const tool = path.basename(process.argv[1]);
const value = flag => args[args.indexOf(flag)+1];
const put = (file, data) => { fs.mkdirSync(path.dirname(file), {recursive:true}); fs.writeFileSync(file,data); };
fs.appendFileSync(process.env.STUB_LOG, JSON.stringify([tool,...args])+'\\n');
if (process.env.STUB_FAIL === tool) process.exit(1);
if (tool === 'rustup') console.log(process.env.STUB_TARGETS);
if (tool === 'cargo') {
  if (!args.includes('--locked')) process.exit(2);
  if (args[0] === 'build') put(path.join(process.env.CARGO_TARGET_DIR,value('--target'),'release/libgitpm_ffi.a'),value('--target'));
  else {
    if (!fs.existsSync(value('--library'))) process.exit(3);
    const out = value('--out-dir');
    put(path.join(out,'Gitpm.swift'),'matching Swift API');
    put(path.join(out,'GitpmFFI.h'),'matching C header');
    put(path.join(out,'GitpmFFI.modulemap'),'module GitpmFFI {}');
  }
}
if (tool === 'lipo' && args[0] === '-create') put(value('-output'),args.slice(1,args.indexOf('-output')).map(f=>fs.readFileSync(f,'utf8')).join('\\n'));
if (tool === 'xcodebuild') {
  const out = value('-output');
  put(path.join(out,'macos-arm64_x86_64/libgitpm_ffi.a'),fs.readFileSync(value('-library')));
  fs.cpSync(value('-headers'),path.join(out,'macos-arm64_x86_64/Headers'),{recursive:true});
  put(path.join(out,'Info.plist'),'stub framework');
}
`;

function fixture(t, {targets='aarch64-apple-darwin\nx86_64-apple-darwin', xcode=true, fail=''}={}) {
  const dir=mkdtempSync(path.join(tmpdir(),'gitpm-apple-'));
  t.after(()=>rmSync(dir,{recursive:true,force:true}));
  mkdirSync(path.join(dir,'scripts'));
  for (const name of ['build-apple.sh','package-apple.sh']) cpSync(path.join(root,'scripts',name),path.join(dir,'scripts',name));
  for (const name of ['LICENSE-MIT','LICENSE-APACHE','README.md']) cpSync(path.join(root,name),path.join(dir,name));
  const bin=path.join(dir,'bin');
  mkdirSync(bin);
  for (const name of ['dirname','grep','rm','mkdir','mktemp','cp','mv','tar','gzip','shasum']) {
    const which=spawnSync('/bin/sh',['-c',`command -v ${name}`],{encoding:'utf8'});
    assert.equal(which.status,0,which.stderr);
    symlinkSync(which.stdout.trim(),path.join(bin,name));
  }
  for (const name of ['rustup','cargo','lipo','gh',...(xcode?['xcodebuild']:[])]) {
    writeFileSync(path.join(bin,name),stub); chmodSync(path.join(bin,name),0o755);
  }
  const env={...process.env, PATH:bin, CARGO_TARGET_DIR:path.join(dir,'target'), STUB_TARGETS:targets,
    STUB_LOG:path.join(dir,'calls.jsonl'), STUB_FAIL:fail, COPYFILE_DISABLE:'1'};
  function run(command, cwd=dir, extra={}) {
    return spawnSync('/bin/bash',['-e','-o','pipefail','-c',command],{cwd,env:{...env,...extra},encoding:'utf8'});
  }
  return {dir,env,run,calls:()=>readFileSync(env.STUB_LOG,'utf8').trim().split('\n').map(JSON.parse)};
}

for (const tag of ['v0.2.0','v0.2.0-rc.1']) test(`workflow packages and uploads matching Apple assets for ${tag}`,t=>{
  const f=fixture(t);
  const apple=job('build-apple');
  const build=apple.match(/run: (scripts\/build-apple[^\n]+)/)[1];
  const pack=apple.match(/run: (scripts\/package-apple[^\n]+)/)[1];
  for (const command of [build,pack]) {
    const result=f.run(command,f.dir,{TAG:tag});
    assert.equal(result.status,0,result.stderr);
  }
  const name=`gitpm-ffi-${tag}-universal-apple-darwin`;
  const dist=path.join(f.dir,'dist');
  const verify=job('upload').match(/run: (sha256sum[^\n]+)/)[1].replace('sha256sum','shasum -a 256');
  assert.equal(f.run(verify,dist).status,0);
  const extracted=path.join(f.dir,'extracted'); mkdirSync(extracted);
  const unpack=f.run(`tar -xzf "${dist}/${name}.tar.gz" -C "${extracted}"`);
  assert.equal(unpack.status,0,unpack.stderr);
  const folder=path.join(extracted,name);
  assert.equal(readFileSync(path.join(folder,'swift/Gitpm.swift'),'utf8'),'matching Swift API');
  assert.equal(readFileSync(path.join(folder,'GitpmFFI.xcframework/macos-arm64_x86_64/libgitpm_ffi.a'),'utf8'),'aarch64-apple-darwin\nx86_64-apple-darwin');
  for (const entry of ['include/GitpmFFI.h','include/module.modulemap','LICENSE-MIT','LICENSE-APACHE','README.md']) assert.ok(existsSync(path.join(folder,entry)),entry);
  const upload=job('upload').match(/run: (gh release upload[^\n]+)/)[1];
  assert.equal(f.run(upload,f.dir,{TAG:tag,GITHUB_REPOSITORY:'test/gitpm'}).status,0);
  const args=f.calls().find(call=>call[0]==='gh');
  assert.deepEqual(args,['gh','release','upload',tag,`dist/${name}.tar.gz`,`dist/${name}.tar.gz.sha256`,'--repo','test/gitpm','--clobber']);
  // A corrupted archive must fail the exact verification command used before upload.
  writeFileSync(path.join(dist,`${name}.tar.gz`),'corrupt');
  assert.notEqual(f.run(verify,dist).status,0);
});

for (const targets of ['aarch64-apple-darwin','x86_64-apple-darwin','']) test(`release rejects missing targets: ${targets || 'both'}`,t=>{
  const f=fixture(t,{targets});
  const result=f.run('scripts/build-apple.sh --release');
  assert.notEqual(result.status,0);
  assert.match(result.stderr,/Release build requires rustup target/);
  assert.deepEqual(f.calls().map(c=>c[0]),['rustup']);
});
test('release rejects missing xcodebuild before compiling',t=>{
  const f=fixture(t,{xcode:false});
  const result=f.run('scripts/build-apple.sh --release');
  assert.notEqual(result.status,0);
  assert.match(result.stderr,/Release build requires xcodebuild/);
  assert.deepEqual(f.calls().map(c=>c[0]),['rustup']);
});
for (const fail of ['cargo','lipo','xcodebuild']) test(`release propagates ${fail} failure`,t=>{
  const f=fixture(t,{fail});
  assert.notEqual(f.run('scripts/build-apple.sh --release').status,0);
});
test('local build keeps single-target and optional Xcode behavior',t=>{
  const f=fixture(t,{targets:'aarch64-apple-darwin',xcode:false});
  const result=f.run('scripts/build-apple.sh');
  assert.equal(result.status,0,result.stderr);
  assert.match(result.stderr,/skipped/);
  assert.equal(readFileSync(path.join(f.dir,'target/apple/lib/libgitpm_ffi.a'),'utf8'),'aarch64-apple-darwin');
  assert.notEqual(f.run('scripts/package-apple.sh v0.2.0').status,0);
});
