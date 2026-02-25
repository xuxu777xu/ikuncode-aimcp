#!/usr/bin/env node

const { spawn } = require("child_process");
const fs = require("fs");
const path = require("path");

const BIN_NAME = process.platform === "win32" ? "aimcp.exe" : "aimcp";
const cargoHome = process.env.CARGO_HOME || path.join(require("os").homedir(), ".cargo");
const binPath = path.join(cargoHome, "bin", BIN_NAME);

if (!fs.existsSync(binPath)) {
  process.stderr.write(
    `[aimcp] Binary not found at ${binPath}\n` +
    "Please run: npm install -g ai-cli-mcp (requires Rust toolchain)\n"
  );
  process.exit(1);
}

const child = spawn(binPath, process.argv.slice(2), {
  stdio: "inherit",
});

child.on("exit", (code) => {
  process.exit(code || 0);
});
