import assert from 'node:assert/strict';
import { test } from 'node:test';
import { codeBlocks } from './code-blocks.mjs';

const pre = (lang: string) => ({
  type: 'element',
  tagName: 'pre',
  properties: { dataLanguage: lang },
  children: [],
});
const nl = { type: 'text', value: '\n' };
const run = (children: object[], path = '/x/content/usage/a.mdx') => {
  const tree = { type: 'root', children };
  codeBlocks()(tree, { path });
  return tree.children as Array<{
    properties: { className: string[] };
    children: unknown[];
  }>;
};

test('adjacent blocks in different languages become one tabbed group', () => {
  const [block, ...rest] = run([pre('js'), nl, pre('python'), nl, pre('go')]);
  assert.deepEqual(block.properties.className, ['code-block', 'code-group']);
  assert.equal(
    block.children.filter((c) => (c as { tagName?: string }).tagName === 'pre')
      .length,
    3,
  );
  assert.equal(rest.length, 0);
});

test('a repeated language starts a new block', () => {
  const blocks = run([pre('js'), nl, pre('js')]).filter((c) => c.properties);
  assert.equal(blocks.length, 2);
  assert.deepEqual(blocks[0].properties.className, ['code-block']);
});

test('the blog keeps plain blocks', () => {
  const [first] = run([pre('js')], '/x/content/blog/post.mdx');
  assert.equal((first as unknown as { tagName: string }).tagName, 'pre');
});
