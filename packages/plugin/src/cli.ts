#!/usr/bin/env node
// rustrak-plugin: new | generate | build | index

import { resolve } from 'node:path';
import { build } from './build.ts';
import { loadConfig } from './config.ts';
import { generate } from './generate.ts';
import { scaffold } from './new.ts';
import { writeIndex } from './registry.ts';

const USAGE = `usage:
  rustrak-plugin new <name>   create a plugin project
  rustrak-plugin generate     write .rustrak/ from rustrak.config.ts and src/messages/
                              (types for the editor, Settings for Rust)
  rustrak-plugin build        compile and pack it into dist/<id>-<version>.rtp
  rustrak-plugin index <dir>  the registry's index.json from the .rtp files in <dir>
                              (sign it: minisign -Sm <dir>/index.json)`;

async function main(argv: string[]): Promise<number> {
  const [command, arg] = argv;
  if (command === 'new' && arg) {
    scaffold(resolve(arg));
    console.log(
      `Created ${arg}. Next:\n  cd ${arg}\n  npm install\n  npm run build`,
    );
    return 0;
  }
  if (command === 'generate') {
    const { warnings } = generate(
      process.cwd(),
      await loadConfig(process.cwd()),
    );
    for (const warning of warnings) console.warn(`warning: ${warning}`);
    return 0;
  }
  if (command === 'index' && arg) {
    console.log(`Wrote ${writeIndex(resolve(arg))}`);
    return 0;
  }
  if (command === 'build') {
    const { rtp } = await build(process.cwd());
    console.log(`Built ${rtp}`);
    return 0;
  }
  console.error(USAGE);
  return 1;
}

main(process.argv.slice(2)).then(
  (code) => process.exit(code),
  (e: Error) => {
    console.error(e.message);
    process.exit(1);
  },
);
