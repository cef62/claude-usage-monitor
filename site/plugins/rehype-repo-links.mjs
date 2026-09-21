// README links are relative to the repo root (CONTRIBUTING.md, LICENSE); the site has no copy
// of those files, so point them at GitHub. Images are left alone: Astro resolves relative
// markdown images itself and bundles them with the site.
const SKIP = /^(https?:\/\/|\/|#|mailto:)/;
const REPO = 'https://github.com/cef62/claude-usage-monitor';

function walk(node) {
  if (node.type === 'element' && node.tagName === 'a') {
    const props = node.properties;
    if (typeof props.href === 'string' && !SKIP.test(props.href)) {
      props.href = `${REPO}/blob/main/${props.href}`;
    }
  }
  for (const child of node.children ?? []) walk(child);
}

export default function rehypeRepoLinks() {
  return (tree) => walk(tree);
}
