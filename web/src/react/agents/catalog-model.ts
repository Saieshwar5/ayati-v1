import { chatRequest } from '../chat/model';

/** A model Cuckoo offers (MG1), under Cuckoo's own name. Who serves it is Cuckoo's
 * choice, so people never see or pick a provider. */
export type CatalogModel = {
  id: string; name: string; family: string; description: string; tags: string[]; context: number;
  supports: { tools: boolean; vision: boolean };
  price: { input: number; output: number; cached_input?: number };
  status: 'available' | 'preview' | 'retiring'; replacement: string | null;
};
/** `catalog_on` is false on servers without a catalog: agents then name a provider and model. */
export type Catalog = { catalog_on: boolean; models: CatalogModel[] };

export const loadCatalog = () => chatRequest<Catalog>('/models');

const dollars = (n: number) => `$${Number(n.toFixed(3))}`;
/** What a model costs, per million tokens read and written; nothing when it isn't priced. */
export function priceLine(model: CatalogModel): string {
  if (!model.price.input && !model.price.output) return '';
  return `${dollars(model.price.input)} in · ${dollars(model.price.output)} out per million tokens`;
}
export function abilities(model: CatalogModel): string[] {
  return [
    model.supports.tools ? 'Uses tools' : '',
    model.supports.vision ? 'Reads images' : '',
    model.context ? `Reads ${Math.round(model.context / 1000)}k tokens at once` : '',
  ].filter(Boolean);
}
export const statusLabel = (model: CatalogModel) => model.status === 'preview' ? 'Preview' : model.status === 'retiring' ? 'Being retired' : '';
/** The catalog models a workspace allows: all of them, or the admin's list. */
export function allowedModels(catalog: Catalog, allowed: { model: string }[] | undefined): CatalogModel[] {
  return allowed?.length ? catalog.models.filter(m => allowed.some(a => a.model === m.id)) : catalog.models;
}
