import { spawn } from 'node:child_process';
import * as path from 'node:path';
import * as url from 'node:url';

const __filename = url.fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Ensure the path points correctly after the TS file is transpiled to `build/services/photos/index.js`
// The rust binary will be in `src/services/photos/rust-cli/target/release/rust-cli` relative to the project root.
const PROJECT_ROOT = path.resolve(__dirname, '..', '..', '..');
const RUST_CLI_PATH = path.join(PROJECT_ROOT, 'src', 'services', 'photos', 'rust-cli', 'target', 'release', 'rust-cli');

export async function runPhotosCli(args: string[], token: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const cliProcess = spawn(RUST_CLI_PATH, args, {
      env: {
        ...process.env,
        GOOGLE_ACCESS_TOKEN: token,
      },
    });

    let stdout = '';
    let stderr = '';

    cliProcess.stdout.on('data', (data: Buffer) => {
      stdout += data.toString();
    });

    cliProcess.stderr.on('data', (data: Buffer) => {
      stderr += data.toString();
    });

    cliProcess.on('close', (code: number | null) => {
      if (code === 0) {
        resolve(stdout);
      } else {
        reject(new Error(`Google Photos CLI failed with code ${code}: ${stderr}`));
      }
    });

    cliProcess.on('error', (err: Error) => {
      reject(new Error(`Failed to spawn Google Photos CLI: ${err.message}`));
    });
  });
}
