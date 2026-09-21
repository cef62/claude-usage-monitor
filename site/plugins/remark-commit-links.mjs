// CHANGELOG bullets from Changesets start "- abc1234: text" (the commit that landed the
// change). Turn the hash prefix into a link to that commit so readers can jump to the diff.
const HASH_PREFIX = /^([0-9a-f]{7}): /;

function walk(node) {
  if (node.type === 'listItem') {
    const paragraph = node.children?.[0];
    const text = paragraph?.type === 'paragraph' ? paragraph.children?.[0] : undefined;
    const match = text?.type === 'text' ? HASH_PREFIX.exec(text.value) : null;
    if (match) {
      const hash = match[1];
      paragraph.children.splice(
        0,
        1,
        {
          type: 'link',
          url: `https://github.com/cef62/claude-usage-monitor/commit/${hash}`,
          children: [{ type: 'text', value: hash }],
        },
        { type: 'text', value: ` ${text.value.slice(match[0].length)}` },
      );
    }
  }
  for (const child of node.children ?? []) walk(child);
}

export default function remarkCommitLinks() {
  return (tree) => walk(tree);
}
