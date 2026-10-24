// engine.mjs · which engine does a local client spawn? Asked of the package
// itself, never assumed: new Nika() resolves its engine when it is built
// (bin, then NIKA_BIN, then the host payload package, never PATH).
// Prints one JSON line: { bin, packageName?, packageRoot? }.
import { Nika } from '@supernovae-st/nika';

const engine = new Nika({ cwd: process.cwd() }).transport?.options?.engine;
if (!engine || typeof engine.bin !== 'string') throw new Error('the package did not name the engine it resolved');
console.log(JSON.stringify(engine));
