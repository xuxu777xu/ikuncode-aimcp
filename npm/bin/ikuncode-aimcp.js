#!/usr/bin/env node

const { spawn } = require("child_process");
const fs = require("fs");
const path = require("path");

const BIN_NAME = process.platform === "win32" ? "ikuncode-aimcp.exe" : "ikuncode-aimcp";

function findBinary() {
  // 1. Check npm package bin dir (downloaded from release)
  const localBin = path.join(__dirname, BIN_NAME);
  if (fs.existsSync(localBin)) return localBin;

  // 2. Check cargo bin dir (installed via cargo install)
  const cargoHome = process.env.CARGO_HOME || path.join(require("os").homedir(), ".cargo");
  const cargoBin = path.join(cargoHome, "bin", BIN_NAME);
  if (fs.existsSync(cargoBin)) return cargoBin;

  return null;
}

const binPath = findBinary();

if (!binPath) {
  const localBinPath = path.join(__dirname, BIN_NAME);
  const cargoHome = process.env.CARGO_HOME || path.join(require("os").homedir(), ".cargo");
  const cargoBinPath = path.join(cargoHome, "bin", BIN_NAME);
  process.stderr.write(
    "[ikuncode-aimcp] Binary not found.\n" +
    `[ikuncode-aimcp]   Platform: ${process.platform}-${process.arch}\n` +
    `[ikuncode-aimcp]   Checked: ${localBinPath}\n` +
    `[ikuncode-aimcp]   Checked: ${cargoBinPath}\n` +
    "[ikuncode-aimcp] Try: npm install -g ikuncode-aimcp --force\n"
  );
  process.exit(1);
}

const child = spawn(binPath, process.argv.slice(2), {
  stdio: "inherit",
});

child.on("exit", (code) => {
  process.exit(code || 0);
});
