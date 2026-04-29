import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitepress";
import { withMermaid } from "vitepress-plugin-mermaid";

// README discovery: auto-generates thin wrapper pages from repo READMEs.
// Sidebar is defined manually below.

type DocPage = {
  source: string; // repo-root-relative path to the README
};

const docsRoot = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = path.resolve(docsRoot, "..");

const IGNORED_DIRS = new Set([
  "node_modules",
  "target",
  "dist",
  ".angular",
  ".vitepress",
  ".git",
  "docs",
  "stls",
  "tests",
  "plan",
]);

function findReadmes(dir: string, out: string[] = []): string[] {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (IGNORED_DIRS.has(entry.name) || entry.name.startsWith(".")) {
        continue;
      }
      findReadmes(path.join(dir, entry.name), out);
    } else if (entry.isFile() && /^README\.md$/i.test(entry.name)) {
      out.push(path.join(dir, entry.name));
    }
  }
  return out;
}

const discovered: Map<string, DocPage> = new Map();
for (const absPath of findReadmes(repoRoot)) {
  const rel = path.relative(repoRoot, absPath).replace(/\\/g, "/");
  discovered.set(rel, { source: rel });
}

// Auto-generate wrapper pages on every config load. Each wrapper is a
// one-line `<!--@include: ... -->` directive pointing at the real README, so
// the docs site stays a thin shell — never a copy.
function writeWrapper(source: string) {
  // Route source → URL using simple rules. Sidebar definitions must match.
  let url: string;
  if (source === "README.md") {
    url = "guide/index";
  } else if (/^[A-Z_]+\.md$/i.test(source)) {
    url = `guide/${source.replace(/\.md$/i, "").toLowerCase()}`;
  } else if (source.startsWith("src/") && source.endsWith("/README.md")) {
    const inner = source.slice("src/".length, -"/README.md".length);
    url = `architecture/${inner.replace(/\//g, "-")}`;
  } else if (source.startsWith("ui/") && source.endsWith("/README.md")) {
    const inner = source.slice("ui/".length, -"/README.md".length);
    url = inner === "" ? "guide/ui" : `guide/ui-${inner.replace(/\//g, "-")}`;
  } else {
    return; // Skip unroutable files
  }

  const wrapperPath = path.join(docsRoot, `${url}.md`);
  const includePath = path
    .relative(path.dirname(wrapperPath), path.join(repoRoot, source))
    .replace(/\\/g, "/");
  const githubUrl = `https://github.com/max-scopp/slicer-engine/blob/main/${source}`;
  const contents = `---
editLink: false
---

> **Source:** [\`${source}\`](${githubUrl}) — this page is rendered directly from the file in the repository. Edit it there.

<!--@include: ${includePath}-->
`;
  fs.mkdirSync(path.dirname(wrapperPath), { recursive: true });
  const existing = fs.existsSync(wrapperPath)
    ? fs.readFileSync(wrapperPath, "utf8")
    : null;
  if (existing !== contents) {
    fs.writeFileSync(wrapperPath, contents);
  }
}

for (const source of discovered.keys()) {
  writeWrapper(source);
}

// https://vitepress.dev/reference/site-config
export default withMermaid(
  defineConfig({
    title: "Slicer Engine",
    description:
      "A high-performance 3D model slicer engine written in Rust, powered by Clipper2.",
    lastUpdated: true,
    cleanUrls: true,

    themeConfig: {
      search: {
        provider: "local",
        options: { detailedView: true },
      },

      nav: [
        { text: "Guide", link: "/guide/", activeMatch: "/guide/" },
        {
          text: "Architecture",
          link: "/architecture/scene",
          activeMatch: "/architecture/",
        },
      ],

      sidebar: {
        "/guide/": [
          { text: "Project Overview", link: "/guide/" },
          { text: "UI", link: "/guide/ui" },
          { text: "Contributing", link: "/guide/contributing" },
        ],
        "/architecture/": [
          { text: "Scene Engine", link: "/architecture/scene" },
          { text: "Mesh", link: "/architecture/mesh" },
          { text: "Arachne", link: "/architecture/arachne" },
          { text: "G-code", link: "/architecture/gcode" },
          { text: "Settings", link: "/architecture/settings" },
          { text: "CLI", link: "/architecture/cli" },
        ],
      },

      socialLinks: [
        {
          icon: "github",
          link: "https://github.com/max-scopp/slicer-engine",
        },
      ],

      outline: { level: [2, 3] },
    },

    // Cross-links between READMEs use filesystem paths that are valid on
    // GitHub but don't match the rendered URLs. Skip the strict check
    // rather than touching upstream README content.
    ignoreDeadLinks: true,

    markdown: {
      lineNumbers: false,
      languages: [
        {
          name: "gcode",
          aliases: ["gc", "nc", "cnc"],
          scopeName: "source.gcode",
          path: path.resolve(docsRoot, ".vitepress/gcode.tmLanguage.json"),
        } as any,
      ],
    },

    vite: {
      optimizeDeps: {
        include: ["mermaid"],
      },
      ssr: {
        noExternal: ["vitepress-plugin-mermaid", "mermaid"],
      },
    },

    mermaid: {
      // Theme automatically follows the VitePress dark/light mode.
    },
  }),
);
