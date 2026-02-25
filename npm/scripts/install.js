const { execSync } = require("child_process");
const fs = require("fs");
const path = require("path");

const REPO_URL = "https://github.com/xuxu777xu/ai-cli-mcp.git";
const BIN_NAME = process.platform === "win32" ? "aimcp.exe" : "aimcp";

function findCargoBin() {
  const cargoHome = process.env.CARGO_HOME || path.join(require("os").homedir(), ".cargo");
  const binPath = path.join(cargoHome, "bin", BIN_NAME);
  if (fs.existsSync(binPath)) {
    return binPath;
  }
  return null;
}

function main() {
  console.log("[aimcp] Installing via cargo install...");

  try {
    execSync("cargo --version", { stdio: "pipe" });
  } catch {
    console.error(
      "[aimcp] Error: Rust toolchain not found.\n" +
      "Please install Rust first: https://rustup.rs\n" +
      "Then run: npm install -g ai-cli-mcp"
    );
    process.exit(1);
  }

  try {
    execSync(`cargo install --git ${REPO_URL} --force`, {
      stdio: "inherit",
    });
  } catch (e) {
    console.error("[aimcp] cargo install failed:", e.message);
    process.exit(1);
  }

  const binPath = findCargoBin();
  if (binPath) {
    console.log(`[aimcp] Installed successfully: ${binPath}`);
  } else {
    console.warn("[aimcp] Warning: binary not found in cargo bin directory");
  }
}

main();
