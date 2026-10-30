// Keep build failures visible in Actions annotations when full logs require sign-in.
import { spawn } from 'node:child_process';

const command = process.argv[2];
if (!command) throw new Error('A workflow command is required');
const child = spawn(command, { shell: true, stdio: ['ignore', 'pipe', 'pipe'] });
let tail = '';
for (const stream of [child.stdout, child.stderr]) {
  stream.on('data', (chunk) => {
    process.stdout.write(chunk);
    tail = (tail + chunk.toString()).slice(-6000);
  });
}
child.on('error', (error) => {
  console.error(error);
  process.exitCode = 1;
});
child.on('close', (code) => {
  if (code !== 0) {
    const message = tail.replace(/%/g, '%25').replace(/\r/g, '%0D').replace(/\n/g, '%0A');
    console.error(`::error title=Release check failed::${message}`);
    process.exitCode = code || 1;
  }
});
