const { execSync } = require("child_process");
const fs = require("fs");
const path = require("path");
const https = require("https");
const http = require("http");
const { createUnzip } = require("zlib");
const { pipeline } = require("stream");

const REPO = "xuxu777xu/ai-cli-mcp";
const REPO_URL = `https://github.com/${REPO}.git`;
const PKG_VERSION = require("../package.json").version;

const PLATFORM_MAP = {
  "win32-x64": { artifact: "ikuncode-aimcp-windows-x86_64.zip", binary: "ikuncode-aimcp.exe" },
  "darwin-x64": { artifact: "ikuncode-aimcp-macos-x86_64.tar.gz", binary: "ikuncode-aimcp" },
  "darwin-arm64": { artifact: "ikuncode-aimcp-macos-aarch64.tar.gz", binary: "ikuncode-aimcp" },
  "linux-x64": { artifact: "ikuncode-aimcp-linux-x86_64.tar.gz", binary: "ikuncode-aimcp" },
};

function getPlatformKey() {
  return `${process.platform}-${process.arch}`;
}

function getBinDir() {
  return path.join(__dirname, "..", "bin");
}

function httpGet(url) {
  return new Promise((resolve, reject) => {
    const client = url.startsWith("https") ? https : http;
    client.get(url, { headers: { "User-Agent": "ikuncode-aimcp-installer" } }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        return httpGet(res.headers.location).then(resolve, reject);
      }
      if (res.statusCode !== 200) {
        return reject(new Error(`HTTP ${res.statusCode} for ${url}`));
      }
      resolve(res);
    }).on("error", reject);
  });
}

async function downloadAndExtract(url, binDir, binaryName) {
  const tmpFile = path.join(binDir, "tmp_download");

  const res = await httpGet(url);
  const chunks = [];
  for await (const chunk of res) {
    chunks.push(chunk);
  }
  const buffer = Buffer.concat(chunks);

  if (url.endsWith(".zip")) {
    fs.writeFileSync(tmpFile, buffer);
    if (process.platform === "win32") {
      execSync(`powershell -Command "Expand-Archive -Path '${tmpFile}' -DestinationPath '${binDir}' -Force"`, { stdio: "pipe" });
    } else {
      execSync(`unzip -o "${tmpFile}" -d "${binDir}"`, { stdio: "pipe" });
    }
    fs.unlinkSync(tmpFile);
  } else {
    fs.writeFileSync(tmpFile, buffer);
    execSync(`tar xzf "${tmpFile}" -C "${binDir}"`, { stdio: "pipe" });
    fs.unlinkSync(tmpFile);
  }

  const binPath = path.join(binDir, binaryName);
  if (process.platform !== "win32" && fs.existsSync(binPath)) {
    fs.chmodSync(binPath, 0o755);
  }
  return binPath;
}

async function tryDownloadRelease() {
  const platformKey = getPlatformKey();
  const info = PLATFORM_MAP[platformKey];
  if (!info) {
    console.log(`[ikuncode-aimcp] No pre-built binary for platform: ${platformKey}`);
    return false;
  }

  const tag = `v${PKG_VERSION}`;
  const url = `https://github.com/${REPO}/releases/download/${tag}/${info.artifact}`;
  const binDir = getBinDir();

  console.log(`[ikuncode-aimcp] Downloading pre-built binary from ${url}`);

  try {
    const binPath = await downloadAndExtract(url, binDir, info.binary);
    if (fs.existsSync(binPath)) {
      console.log(`[ikuncode-aimcp] Installed successfully: ${binPath}`);
      return true;
    }
  } catch (e) {
    console.log(`[ikuncode-aimcp] Download failed: ${e.message}`);
  }
  return false;
}

function tryCargoInstall() {
  console.log("[ikuncode-aimcp] Falling back to cargo install...");

  try {
    execSync("cargo --version", { stdio: "pipe" });
  } catch {
    console.error(
      "[ikuncode-aimcp] Error: No pre-built binary available and Rust toolchain not found.\n" +
      "Please install Rust first: https://rustup.rs\n" +
      "Then run: npm install -g ikuncode-aimcp"
    );
    process.exit(1);
  }

  try {
    execSync(`cargo install --git ${REPO_URL} --force`, { stdio: "inherit" });
    console.log("[ikuncode-aimcp] Installed via cargo install");
  } catch (e) {
    console.error("[ikuncode-aimcp] cargo install failed:", e.message);
    process.exit(1);
  }
}

async function main() {
  const binDir = getBinDir();
  if (!fs.existsSync(binDir)) {
    fs.mkdirSync(binDir, { recursive: true });
  }

  const downloaded = await tryDownloadRelease();
  if (!downloaded) {
    tryCargoInstall();
  }
}

main();
