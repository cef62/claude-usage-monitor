// The site renders README.md and CHANGELOG.md inside its own pages, which carry their own
// title. Drop each file's preamble — the `# ` title, and for the README the intro and
// screenshot tables that belong to the landing page — so content starts at the first `## `.
export default function remarkDropPreamble() {
  return (tree) => {
    const start = tree.children.findIndex((n) => n.type === 'heading' && n.depth === 2);
    if (start > 0) tree.children.splice(0, start);
  };
}
