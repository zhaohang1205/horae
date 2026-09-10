const fs = require('fs');
const path = require('path');
const https = require('https');
const http = require('http');
const { execFileSync } = require('child_process');

const pkg = require('../package.json');

function isMusl() {
  try {
    const report = process.report?.getReport();
    if (report && report.header && report.header.glibcVersionRuntime) {
      return false;
    }
  } catch (_) {}
  return (
    fs.existsSync('/lib/ld-musl-x86_64.so.1') ||
    fs.existsSync('/lib/ld-musl-aarch64.so.1')
  );
}

function getTarget() {
  const platform = process.platform;
  const arch = process.arch;

  if (platform === 'linux' && arch === 'x64') {
    return isMusl() ? 'x86_64-unknown-linux-musl' : 'x86_64-unknown-linux-gnu';
  }
  if (platform === 'darwin') {
    if (arch === 'arm64') return 'aarch64-apple-darwin';
    if (arch === 'x64') return 'x86_64-apple-darwin';
  }
  if (platform === 'win32' && arch === 'x64') {
    return 'x86_64-pc-windows-msvc';
  }

  return null;
}

function getBinaryName() {
  return process.platform === 'win32' ? 'horae.exe' : 'horae';
}

function getBinaryPath() {
  return path.join(__dirname, '..', 'bin', getBinaryName());
}

function downloadFile(url, dest, maxRedirects = 5) {
  return new Promise((resolve, reject) => {
    if (maxRedirects <= 0) {
      return reject(new Error(`Too many redirects when downloading ${url}`));
    }

    const client = url.startsWith('https') ? https : http;
    const req = client.get(url, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        const redirectUrl = new URL(res.headers.location, url).toString();
        return resolve(downloadFile(redirectUrl, dest, maxRedirects - 1));
      }

      if (res.statusCode !== 200) {
        return reject(new Error(`Download failed with HTTP ${res.statusCode}: ${url}`));
      }

      const file = fs.createWriteStream(dest);
      res.pipe(file);
      file.on('finish', () => {
        file.close(() => resolve());
      });
      file.on('error', (err) => {
        fs.unlink(dest, () => {});
        reject(err);
      });
    });

    req.on('error', (err) => {
      fs.unlink(dest, () => {});
      reject(err);
    });
  });
}

function extractArchive(archivePath, destDir, isZip) {
  if (isZip) {
    try {
      execFileSync('tar.exe', ['-xf', archivePath, '-C', destDir], { stdio: 'ignore' });
      return;
    } catch (_) {
      execFileSync(
        'powershell.exe',
        [
          '-NoProfile',
          '-NonInteractive',
          '-Command',
          `Expand-Archive -Path "${archivePath}" -DestinationPath "${destDir}" -Force`
        ],
        { stdio: 'inherit' }
      );
      return;
    }
  }

  execFileSync('tar', ['-xzf', archivePath, '-C', destDir], { stdio: 'inherit' });
}

async function install() {
  const target = getTarget();
  if (!target) {
    console.warn(
      `[horae-cli] Notice: Prebuilt binary is not available for ${process.platform}-${process.arch}.\n` +
      `[horae-cli] You can install horae directly from source using Cargo: cargo install horae`
    );
    return;
  }

  const binDir = path.join(__dirname, '..', 'bin');
  if (!fs.existsSync(binDir)) {
    fs.mkdirSync(binDir, { recursive: true });
  }

  const binaryPath = getBinaryPath();
  if (fs.existsSync(binaryPath)) {
    return;
  }

  const isZip = target.includes('windows');
  const ext = isZip ? 'zip' : 'tar.gz';
  const version = process.env.HORAE_VERSION || pkg.version;
  const filename = `horae-v${version}-${target}.${ext}`;
  const baseUrl = (process.env.HORAE_MIRROR || 'https://github.com').replace(/\/+$/, '');
  const downloadUrl = `${baseUrl}/zhaohang1205/horae/releases/download/v${version}/${filename}`;

  const tempArchive = path.join(binDir, `temp-${filename}`);

  console.log(`[horae-cli] Downloading horae v${version} for ${target}...`);
  try {
    await downloadFile(downloadUrl, tempArchive);
    console.log(`[horae-cli] Extracting ${filename}...`);
    extractArchive(tempArchive, binDir, isZip);

    if (process.platform !== 'win32') {
      fs.chmodSync(binaryPath, 0o755);
    }
    console.log(`[horae-cli] horae binary installed successfully!`);
  } catch (err) {
    console.error(`[horae-cli] Error: ${err.message}`);
    throw err;
  } finally {
    if (fs.existsSync(tempArchive)) {
      try {
        fs.unlinkSync(tempArchive);
      } catch (_) {}
    }
  }
}

function ensureInstalled() {
  const binaryPath = getBinaryPath();
  if (fs.existsSync(binaryPath)) {
    return;
  }
  execFileSync(process.execPath, [path.join(__dirname, 'install.js')], {
    stdio: 'inherit'
  });
}

if (require.main === module) {
  install().catch((err) => {
    console.error(`[horae-cli] Postinstall failed: ${err.message}`);
    process.exit(0);
  });
}

module.exports = {
  getTarget,
  getBinaryName,
  getBinaryPath,
  install,
  ensureInstalled
};
