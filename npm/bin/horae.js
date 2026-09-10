#!/usr/bin/env node

const fs = require('fs');
const { spawnSync } = require('child_process');
const { ensureInstalled, getBinaryPath } = require('../lib/install');

const binPath = getBinaryPath();

if (!fs.existsSync(binPath)) {
  try {
    ensureInstalled();
  } catch (err) {
    console.error(`[horae-cli] Failed to prepare horae binary: ${err.message}`);
    process.exit(1);
  }
}

const result = spawnSync(binPath, process.argv.slice(2), {
  stdio: 'inherit'
});

if (result.error) {
  console.error(`[horae-cli] Execution error:`, result.error);
  process.exit(1);
}

process.exit(result.status ?? 0);
