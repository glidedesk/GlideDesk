#!/usr/bin/env node
// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
'use strict';

const { spawnSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const ROOT = path.resolve(__dirname, '..');
const args = process.argv.slice(2);

const skipAndroid = args.includes('--no-android') || args.includes('--fast') || args.includes('--js') || args.includes('--rust');
const runRustOnly = args.includes('--rust');
const runJsOnly = args.includes('--js');

console.log('\n\x1b[1m\x1b[36m============================================================\x1b[0m');
console.log('\x1b[1m\x1b[36m             GlideDesk Unified Test Suite                   \x1b[0m');
console.log('\x1b[1m\x1b[36m============================================================\x1b[0m\n');

let totalPassed = 0;
let totalFailed = 0;
const results = [];

// 1. Rust Engine Tests
if (!runJsOnly) {
  console.log('\x1b[1m\x1b[33m[1/3] Running Rust Engine & KVM Tests (cargo test)...\x1b[0m');
  const rustStart = Date.now();
  const rustRes = spawnSync('cargo', ['test', '--manifest-path', path.join(ROOT, 'src-tauri', 'Cargo.toml')], {
    stdio: 'inherit',
    cwd: ROOT,
  });
  const rustDuration = ((Date.now() - rustStart) / 1000).toFixed(2);

  if (rustRes.status === 0) {
    console.log(`\x1b[32m✔ Rust Backend tests passed (${rustDuration}s)\x1b[0m\n`);
    results.push({ name: 'Rust Engine & KVM Backend', status: 'PASS', duration: `${rustDuration}s` });
    totalPassed++;
  } else {
    console.error(`\x1b[31m✖ Rust Backend tests failed (exit code ${rustRes.status})\x1b[0m\n`);
    results.push({ name: 'Rust Engine & KVM Backend', status: 'FAIL', duration: `${rustDuration}s` });
    totalFailed++;
  }
}

// 2. JavaScript Protocol & Mesh Tests
if (!runRustOnly) {
  console.log('\x1b[1m\x1b[33m[2/3] Running Frontend & Wire Protocol Tests (node ui/shared/test/*.js)...\x1b[0m');
  const testDir = path.join(ROOT, 'ui', 'shared', 'test');
  const testFiles = fs.readdirSync(testDir).filter((f) => f.endsWith('.js')).sort();
  let jsPass = 0;
  let jsFail = 0;

  for (const f of testFiles) {
    const fullPath = path.join(testDir, f);
    const start = Date.now();
    const res = spawnSync(process.execPath, [fullPath], {
      stdio: ['pipe', 'pipe', 'pipe'],
      cwd: ROOT,
    });
    const dur = Date.now() - start;

    if (res.status === 0) {
      console.log(`  \x1b[32m✔\x1b[0m ${f.padEnd(25)} \x1b[90m(${dur}ms)\x1b[0m`);
      jsPass++;
    } else {
      console.error(`  \x1b[31m✖\x1b[0m ${f.padEnd(25)} \x1b[31mFAILED\x1b[0m`);
      const stderr = res.stderr.toString().trim() || res.stdout.toString().trim();
      console.error(`    \x1b[31m${stderr.split('\n').slice(0, 5).join('\n    ')}\x1b[0m`);
      jsFail++;
    }
  }

  if (jsFail === 0) {
    console.log(`\x1b[32m✔ All ${jsPass} JS protocol & mesh test suites passed\x1b[0m\n`);
    results.push({ name: `JS Protocol & State Machine (${jsPass} suites)`, status: 'PASS' });
    totalPassed++;
  } else {
    console.error(`\x1b[31m✖ ${jsFail}/${jsPass + jsFail} JS test suites failed\x1b[0m\n`);
    results.push({ name: `JS Protocol & State Machine (${jsFail} failed)`, status: 'FAIL' });
    totalFailed++;
  }
}

// 3. Android Companion Tests
if (!skipAndroid && !runRustOnly && !runJsOnly) {
  const gradlew = process.platform === 'win32' ? 'gradlew.bat' : './gradlew';
  const androidDir = path.join(ROOT, 'android');

  if (fs.existsSync(path.join(androidDir, gradlew))) {
    console.log('\x1b[1m\x1b[33m[3/3] Running Android Companion Tests (./gradlew test)...\x1b[0m');
    const andStart = Date.now();
    const andRes = spawnSync(gradlew, ['test'], {
      stdio: 'inherit',
      cwd: androidDir,
    });
    const andDuration = ((Date.now() - andStart) / 1000).toFixed(2);

    if (andRes.status === 0) {
      console.log(`\x1b[32m✔ Android Companion tests passed (${andDuration}s)\x1b[0m\n`);
      results.push({ name: 'Android Mobile Companion', status: 'PASS', duration: `${andDuration}s` });
      totalPassed++;
    } else {
      console.error(`\x1b[31m✖ Android Companion tests failed\x1b[0m\n`);
      results.push({ name: 'Android Mobile Companion', status: 'FAIL', duration: `${andDuration}s` });
      totalFailed++;
    }
  } else {
    console.log('\x1b[90m[-] Android gradlew not found, skipping Android tests.\x1b[0m\n');
  }
}

// Summary Report
console.log('\x1b[1m\x1b[36m============================================================\x1b[0m');
console.log('\x1b[1m\x1b[36m                     Test Summary Report                    \x1b[0m');
console.log('\x1b[1m\x1b[36m============================================================\x1b[0m');
for (const r of results) {
  const icon = r.status === 'PASS' ? '\x1b[32m[PASS]\x1b[0m' : '\x1b[31m[FAIL]\x1b[0m';
  const dur = r.duration ? `\x1b[90m(${r.duration})\x1b[0m` : '';
  console.log(` ${icon} ${r.name.padEnd(42)} ${dur}`);
}
console.log('\x1b[1m\x1b[36m------------------------------------------------------------\x1b[0m');

if (totalFailed === 0) {
  console.log('\x1b[1m\x1b[32m ALL TEST SUITES PASSED! \x1b[0m\n');
  process.exit(0);
} else {
  console.log(`\x1b[1m\x1b[31m ${totalFailed} TEST SUITE(S) FAILED. \x1b[0m\n`);
  process.exit(1);
}
