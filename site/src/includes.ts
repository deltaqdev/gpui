import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Loader } from 'astro/loaders';
import { markdownToMdast, type MdastContent, type MdastPluginDefinition } from 'satteri';

// A fenced block whose language is `include` names one file, relative to the
// page, that is spliced in at build time:
//
//     ```include
//     ../../../../../crates/gpui/README.md
//     ```
//
// A language after `include` wraps the file in a code block of that language
// instead of parsing it as Markdown:
//
//     ```include toml
//     ../../../../../deltaq/changes.toml
//     ```
//
// Upstream files (crates/gpui/README.md, crates/gpui/docs/*.md) and fork files
// (deltaq/*) are read where they live, so the site never carries a copy that
// could drift. The same directive is expanded twice: by `includePlugin` when a
// page is rendered to HTML, and by `withIncludes` for the raw-Markdown routes
// (`<page>.md`, `llms-full.txt`), which serve the collection entry's body.

const INCLUDE_FENCE = /^```include(?:[ \t]+([\w-]+))?[ \t]*\n([^\n`]+)\n```[ \t]*$/gm;

// The included file's own H1 would repeat the page title, which Mines renders
// from frontmatter.
const LEADING_H1 = /^#[ \t][^\n]*\n+/;

function readInclude(pagePath: string, target: string, lang: string | undefined): string {
  const content = readFileSync(resolve(dirname(pagePath), target.trim()), 'utf8').trimEnd();
  return lang ? content : content.replace(LEADING_H1, '');
}

function fence(lang: string, content: string): string {
  return '```' + lang + '\n' + content + '\n```';
}

export function expandIncludes(source: string, pagePath: string): string {
  return source.replace(INCLUDE_FENCE, (_match, lang: string | undefined, target: string) => {
    const content = readInclude(pagePath, target, lang);
    return lang ? fence(lang, content) : content;
  });
}

export const includePlugin: MdastPluginDefinition = {
  name: 'include',
  code(node, ctx) {
    if (node.lang !== 'include') return;
    if (!ctx.fileURL) throw new Error('include: the page being compiled has no fileURL');
    const lang = node.meta?.trim() || undefined;
    const content = readInclude(fileURLToPath(ctx.fileURL), node.value, lang);
    if (lang) {
      ctx.replaceNode(node, { type: 'code', lang, value: content });
      return;
    }
    const root = markdownToMdast(content, { position: false });
    if (root.type !== 'root') throw new Error('include: expected a root node');
    ctx.replaceNode(node, root.children as MdastContent[]);
  },
};

export function withIncludes(loader: Loader): Loader {
  return {
    ...loader,
    name: `${loader.name}+includes`,
    async load(context) {
      await loader.load(context);
      const root = fileURLToPath(context.config.root);
      for (const entry of context.store.values()) {
        if (!entry.filePath || !entry.body) continue;
        const body = expandIncludes(entry.body, resolve(root, entry.filePath));
        if (body === entry.body) continue;
        // The store skips a `set` whose digest matches the stored entry.
        context.store.set({ ...entry, body, digest: context.generateDigest(body) });
      }
    },
  };
}
