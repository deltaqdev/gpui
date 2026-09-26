import { defineConfig } from 'astro/config';
import { satteri } from '@astrojs/markdown-satteri';
import mines from '@deltaq.dev/mines';
import { includePlugin } from './src/includes.ts';

export default defineConfig({
  site: 'https://gpui.deltaq.dev',
  markdown: {
    processor: satteri({ mdastPlugins: [includePlugin] }),
  },
  integrations: [
    mines({
      title: 'GPUI at DeltaQ',
      description: 'How DeltaQ developers build on GPUI, and how this fork tracks Zed',
      github: 'deltaqdev/gpui',
      editUrl: 'https://github.com/deltaqdev/gpui/edit/gpui/site/',
      groups: { upstream: 'Upstream reference', fork: 'This fork' },
    }),
  ],
});
