// Liemo's own case: one coast reel, asked for in words, cut on the timeline.
import type { FilmContent } from './kit/screens';

export const CONTENT: FilmContent = {
  meta: {
    product: 'Liemo',
    does: 'edits video on a Mac, by hand or with an agent',
    domain: 'editor',
    date: '2026-11-13',
    weekday: 'Friday',
    now: '3:00 PM',
    seed: 'Liemo',
    currency: '$',
  },
  people: [],
  prompt: {
    text: 'Cut a coast reel called Summer, slowly',
    placeholder: 'Ask for a cut',
    model: 'Agent',
  },
  url: {
    url: 'github.com/flaxodotdev/liemo',
    action: 'View on GitHub',
    busy: 'Opening',
    done: 'Open',
    placeholder: 'github.com/flaxodotdev/liemo',
  },
  profile: {
    name: 'Liemo',
    handle: 'liemo',
    verified: false,
    stats: ['Mac', 'Open source'],
    action: 'Liemo is out now',
    done: 'Ready',
  },
  thumbs: [
    { title: 'Coastal cliffs', meta: '8s' },
    { title: 'Canyon sunset', meta: '7s' },
    { title: 'Forest waterfall', meta: '6s' },
  ],
  comments: { comments: [], header: { label: 'Notes', total: 0 } },
  notices: {
    items: [
      { title: 'Coastal cliffs', body: 'On the timeline', ago: 'now', glyph: 'play' },
      { title: 'Canyon sunset', body: 'On the timeline', ago: 'now', glyph: 'play' },
      { title: 'Summer, slowly', body: 'Title on the frame', ago: 'now', glyph: 'check' },
    ],
  },
  payments: {
    items: [
      { title: 'Coastal cliffs', value: '8' },
      { title: 'Canyon sunset', value: '7' },
      { title: 'Forest waterfall', value: '6' },
    ],
    header: { title: 'Reel length', value: 21, money: true, currency: '$' },
  },
  products: {
    items: [
      { title: 'Coastal cliffs', sub: 'Main video' },
      { title: 'Canyon sunset', sub: 'Main video' },
      { title: 'Forest waterfall', sub: 'Main video' },
    ],
  },
  steps: {
    title: 'Summer, slowly',
    sub: 'Three clips',
    steps: [
      { label: 'Coastal cliffs', meta: '8s' },
      { label: 'Canyon sunset', meta: '7s' },
      { label: 'Forest waterfall', meta: '6s' },
    ],
  },
  progress: { label: 'Summer, slowly', to: 3 },
  plans: {
    currency: '$',
    plans: [
      { name: 'Mac', price: 0, features: ['Timeline', 'Effects', 'Export'], cta: 'Liemo is out now' },
    ],
  },
  doc: {
    brand: 'Liemo',
    title: 'Summer, slowly',
    lead: 'A coast reel cut on the timeline.',
    section: 'Clips',
    body: ['Coastal cliffs opens the reel.', 'Canyon sunset follows it.', 'Forest waterfall closes it.'],
    table: {
      head: ['Clip', 'Track', 'Length'],
      rows: [
        ['Coastal cliffs', 'Main', '8s'],
        ['Canyon sunset', 'Main', '7s'],
        ['Forest waterfall', 'Main', '6s'],
      ],
    },
  },
  code: {
    file: 'session',
    lang: 'text',
    tabs: ['session'],
    code: 'add_clip asset sc-coast\nadd_text Summer, slowly\nexport',
  },
  reply: {
    ask: 'Cut a coast reel called Summer, slowly',
    who: { name: 'Liemo' },
    text: 'Three clips are on the timeline.',
    items: [{ text: 'Coastal cliffs' }, { text: 'Canyon sunset' }, { text: 'Forest waterfall' }],
    close: 'Summer, slowly',
  },
};
