import { defineCollection } from 'astro:content';
import { docsLoader, docsSchema } from '@deltaq.dev/mines/loader';
import { withIncludes } from './includes';

export const collections = {
  docs: defineCollection({ loader: withIncludes(docsLoader()), schema: docsSchema() }),
};
