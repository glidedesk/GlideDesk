// GlideDesk — Bridge Parity Verification Script
const fs = require('fs');
const path = require('path');

const bridgePath = path.join(__dirname, '../ui/shared/tauri-bridge.js');
const mainPath = path.join(__dirname, '../src-tauri/src/main.rs');

const bridgeSrc = fs.readFileSync(bridgePath, 'utf8');
const mainSrc = fs.readFileSync(mainPath, 'utf8');

const invokeMatches = [...bridgeSrc.matchAll(/invoke\(['"]([a-zA-Z0-9_]+)['"]/g)].map(m => m[1]);
const uniqueInvokes = [...new Set(invokeMatches)].sort();

const registeredCommands = [...mainSrc.matchAll(/(?:[a-zA-Z0-9_]+::)+([a-zA-Z0-9_]+)/g)].map(m => m[1]);
const uniqueCommands = new Set(registeredCommands);

console.log(`Checking ${uniqueInvokes.length} invoked bridge commands against Tauri registrations...`);

let missing = [];
for (const cmd of uniqueInvokes) {
  if (!uniqueCommands.has(cmd)) {
    missing.push(cmd);
  }
}

if (missing.length > 0) {
  console.error(`FAILED: The following invoked commands are not registered in main.rs:`, missing);
  process.exit(1);
} else {
  console.log(`SUCCESS: All ${uniqueInvokes.length} bridge invoke() commands are registered in main.rs!`);
}
