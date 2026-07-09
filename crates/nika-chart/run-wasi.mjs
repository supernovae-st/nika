import { readFileSync, writeFileSync } from 'node:fs';
import { WASI } from 'node:wasi';
const wasi = new WASI({ version: 'preview1', args: ['parity'] });
const wasm = await WebAssembly.compile(readFileSync('./target-session/wasm32-wasip1/release/examples/parity.wasm'));
const instance = await WebAssembly.instantiate(wasm, wasi.getImportObject());
wasi.start(instance);
