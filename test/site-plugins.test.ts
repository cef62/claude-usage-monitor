import { describe, expect, it } from 'vitest';
import rehypeRepoLinks from '../site/plugins/rehype-repo-links.mjs';
import remarkCommitLinks from '../site/plugins/remark-commit-links.mjs';

// Minimal mdast-shaped nodes — just enough structure for remark-commit-links to walk.
interface TextNode {
  type: 'text';
  value: string;
}
interface InlineCodeNode {
  type: 'inlineCode';
  value: string;
}
interface LinkNode {
  type: 'link';
  url: string;
  children: TextNode[];
}
interface ParagraphNode {
  type: 'paragraph';
  children: (TextNode | LinkNode | InlineCodeNode)[];
}
interface ListItemNode {
  type: 'listItem';
  children: ParagraphNode[];
}
interface ListNode {
  type: 'list';
  children: ListItemNode[];
}
interface BlockquoteNode {
  type: 'blockquote';
  children: ListNode[];
}
interface MdastRoot {
  type: 'root';
  children: (ListNode | BlockquoteNode)[];
}

describe('remarkCommitLinks', () => {
  it('turns a "- abc1234: text" bullet into a commit link followed by the remainder', () => {
    const paragraph: ParagraphNode = {
      type: 'paragraph',
      children: [{ type: 'text', value: 'abc1234: Fixed a bug in the poller.' }],
    };
    const tree: MdastRoot = {
      type: 'root',
      children: [{ type: 'list', children: [{ type: 'listItem', children: [paragraph] }] }],
    };

    remarkCommitLinks()(tree);

    expect(paragraph.children[0]).toEqual({
      type: 'link',
      url: 'https://github.com/cef62/claude-usage-monitor/commit/abc1234',
      children: [{ type: 'text', value: 'abc1234' }],
    });
    expect(paragraph.children[1]).toEqual({
      type: 'text',
      value: ' Fixed a bug in the poller.',
    });
  });

  it('leaves list items with no commit-hash prefix untouched', () => {
    const paragraph: ParagraphNode = {
      type: 'paragraph',
      children: [{ type: 'text', value: 'Just a note.' }],
    };
    const tree: MdastRoot = {
      type: 'root',
      children: [{ type: 'list', children: [{ type: 'listItem', children: [paragraph] }] }],
    };

    remarkCommitLinks()(tree);

    expect(paragraph.children).toEqual([{ type: 'text', value: 'Just a note.' }]);
  });

  it('leaves later inline-formatting siblings (e.g. inline code) untouched', () => {
    const inlineCode: InlineCodeNode = { type: 'inlineCode', value: 'code' };
    const paragraph: ParagraphNode = {
      type: 'paragraph',
      children: [{ type: 'text', value: 'abc1234: text with ' }, inlineCode],
    };
    const tree: MdastRoot = {
      type: 'root',
      children: [{ type: 'list', children: [{ type: 'listItem', children: [paragraph] }] }],
    };

    remarkCommitLinks()(tree);

    expect(paragraph.children[0]).toEqual({
      type: 'link',
      url: 'https://github.com/cef62/claude-usage-monitor/commit/abc1234',
      children: [{ type: 'text', value: 'abc1234' }],
    });
    expect(paragraph.children[1]).toEqual({ type: 'text', value: ' text with ' });
    // The inlineCode node must survive the splice unchanged, at the tail of the array.
    expect(paragraph.children[2]).toBe(inlineCode);
  });

  it('walks nested structures to find list items anywhere in the tree', () => {
    const paragraph: ParagraphNode = {
      type: 'paragraph',
      children: [{ type: 'text', value: '0123abc: Nested change.' }],
    };
    const tree: MdastRoot = {
      type: 'root',
      children: [
        {
          type: 'blockquote',
          children: [{ type: 'list', children: [{ type: 'listItem', children: [paragraph] }] }],
        },
      ],
    };

    remarkCommitLinks()(tree);

    const link = paragraph.children[0];
    expect(link?.type).toBe('link');
    expect((link as LinkNode).url).toBe(
      'https://github.com/cef62/claude-usage-monitor/commit/0123abc',
    );
  });
});

// Minimal hast-shaped nodes — just enough structure for rehype-repo-links to walk.
interface HastElement {
  type: 'element';
  tagName: string;
  properties: Record<string, string>;
  children: HastElement[];
}
interface HastRoot {
  type: 'root';
  children: HastElement[];
}

describe('rehypeRepoLinks', () => {
  it('rewrites a relative img src to a raw.githubusercontent.com URL', () => {
    const img: HastElement = {
      type: 'element',
      tagName: 'img',
      properties: { src: 'docs/screenshots/mac-tray.png' },
      children: [],
    };
    const tree: HastRoot = { type: 'root', children: [img] };

    rehypeRepoLinks()(tree);

    expect(img.properties.src).toBe(
      'https://raw.githubusercontent.com/cef62/claude-usage-monitor/main/docs/screenshots/mac-tray.png',
    );
  });

  it('rewrites a relative a href to a github.com blob URL', () => {
    const a: HastElement = {
      type: 'element',
      tagName: 'a',
      properties: { href: 'CONTRIBUTING.md' },
      children: [],
    };
    const tree: HastRoot = { type: 'root', children: [a] };

    rehypeRepoLinks()(tree);

    expect(a.properties.href).toBe(
      'https://github.com/cef62/claude-usage-monitor/blob/main/CONTRIBUTING.md',
    );
  });

  it('leaves absolute, anchor, and mailto links untouched', () => {
    const make = (href: string): HastElement => ({
      type: 'element',
      tagName: 'a',
      properties: { href },
      children: [],
    });
    const links = [
      make('https://example.com'),
      make('http://example.com'),
      make('/local'),
      make('#section'),
      make('mailto:me@example.com'),
    ];
    const tree: HastRoot = { type: 'root', children: links };

    rehypeRepoLinks()(tree);

    expect(links.map((n) => n.properties.href)).toEqual([
      'https://example.com',
      'http://example.com',
      '/local',
      '#section',
      'mailto:me@example.com',
    ]);
  });

  it('recurses into element children to find nested img/a nodes', () => {
    const a: HastElement = {
      type: 'element',
      tagName: 'a',
      properties: { href: 'LICENSE' },
      children: [],
    };
    const p: HastElement = { type: 'element', tagName: 'p', properties: {}, children: [a] };
    const tree: HastRoot = { type: 'root', children: [p] };

    rehypeRepoLinks()(tree);

    expect(a.properties.href).toBe(
      'https://github.com/cef62/claude-usage-monitor/blob/main/LICENSE',
    );
  });
});
