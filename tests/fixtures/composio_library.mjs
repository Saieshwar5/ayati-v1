// The library part of the Composio stand-in: three apps and HubSpot's tools,
// shaped like Composio's answers, for the added-apps journey.
const VERSION = '20260901_00';
const kits = {
  hubspot: { name: 'HubSpot', managed: ['OAUTH2'], about: 'Customers, deals and marketing. Used by sales teams.', count: 4 },
  docker_hub: { name: 'Docker Hub', managed: [], about: 'Container images.', count: 12 },
  gmail: { name: 'Gmail', managed: ['OAUTH2'], about: 'Email.', count: 61 },
};
const tools = {
  HUBSPOT_CREATE_CONTACTS: { name: 'Create contacts in bulk', tags: ['createHint'], input: { type: 'object', properties: { records: { type: 'array', items: { type: 'object' } } }, required: ['records'] } },
  HUBSPOT_CREATE_NOTE: { name: 'Create note', tags: ['createHint'], input: { type: 'object', properties: { body: { type: 'string', description: 'The note' }, contact_id: { type: 'string' }, pinned: { type: 'boolean' } }, required: ['body'] } },
  HUBSPOT_PING: { name: 'Check the account', tags: [], input: { type: 'object', properties: {} } },
  HUBSPOT_SEARCH_CONTACTS: { name: 'Search contacts', tags: ['readOnlyHint'], input: { type: 'object', properties: { query: { type: 'string', description: 'Name or email' }, limit: { type: 'integer' }, filters: { type: 'array', items: { type: 'object' } } }, required: ['query'] } },
};
const kit = slug => kits[slug] && { slug, name: kits[slug].name, no_auth: false, composio_managed_auth_schemes: kits[slug].managed, auth_schemes: ['OAUTH2'],
  meta: { description: kits[slug].about, tools_count: kits[slug].count, categories: [{ name: 'CRM', slug: 'crm' }] } };

/** A HubSpot tool as Composio lists and returns it, or undefined. */
export const libraryTool = slug => tools[slug] && { slug, name: tools[slug].name, description: `${tools[slug].name} in HubSpot. More words.`,
  toolkit: { slug: 'hubspot', name: 'hubspot', logo: '' }, input_parameters: tools[slug].input, output_parameters: {}, no_auth: false,
  available_versions: [VERSION], version: VERSION, scopes: [], tags: tools[slug].tags, is_deprecated: false };

/** Answers a library request (`api` is the path after /api/v3.1/), or returns false. */
export function library(api, url, send) {
  if (api === 'toolkits') {
    const search = (url.searchParams.get('search') ?? '').toLowerCase();
    const items = Object.keys(kits).filter(slug => slug.includes(search)).map(kit);
    send({ items, next_cursor: null, total_items: items.length }); return true;
  }
  if (api.startsWith('toolkits/')) { const found = kit(api.slice(9)); send(found ?? {}, found ? 200 : 404); return true; }
  if (api === 'tools') {
    const search = (url.searchParams.get('search') ?? '').toUpperCase().replaceAll(' ', '_');
    const found = url.searchParams.get('toolkit_slug') === 'hubspot' ? Object.keys(tools).filter(slug => slug.includes(search)) : [];
    const start = url.searchParams.get('cursor') === 'Mi0y' ? 2 : 0;
    send({ items: found.slice(start, start + 2).map(libraryTool), next_cursor: start === 0 && found.length > 2 ? 'Mi0y' : null, total_items: found.length }); return true;
  }
  return false;
}
