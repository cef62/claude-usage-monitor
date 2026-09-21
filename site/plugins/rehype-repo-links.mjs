// README image/link paths are relative to the repo root; GitHub resolves those, but the static
// site has no copy of the files, so point them at GitHub's raw/blob URLs instead.
const SKIP = /^(https?:\/\/|\/|#|mailto:)/;
const REPO = 'https://github.com/cef62/claude-usage-monitor';

function walk(node) {
  if (node.type === 'element') {
    const props = node.properties ?? {};
    if (node.tagName === 'img' && typeof props.src === 'string' && !SKIP.test(props.src)) {
      props.src = `${REPO.replace('github.com', 'raw.githubusercontent.com')}/main/${props.src}`;
    }
    if (node.tagName === 'a' && typeof props.href === 'string' && !SKIP.test(props.href)) {
      props.href = `${REPO}/blob/main/${props.href}`;
    }
  }
  for (const child of node.children ?? []) walk(child);
}

export default function rehypeRepoLinks() {
  return (tree) => walk(tree);
}
