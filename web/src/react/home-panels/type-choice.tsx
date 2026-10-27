import { CHART_VARIANTS, PANEL_FAMILIES, type ChartVariant, type PanelFamily } from '../../panel-types';
export type PanelChoice = { family: PanelFamily | ''; variant: ChartVariant };
export function panelPreference(choice: PanelChoice) {
  return choice.family ? `\nRequested UI family: ${choice.family}${choice.family === 'chart' ? `; chart variant: ${choice.variant}` : ''}. Keep the UI general and configure it for my data.` : '';
}
export function PanelTypeChoice({ id, value, change, disabled }: {
  id: string; value: PanelChoice; change: (value: PanelChoice) => void; disabled: boolean;
}) {
  return <>
    <div className="workspace-panel-field">
      <label htmlFor={`${id}-family`}>Panel type</label>
      <select id={`${id}-family`} value={value.family} disabled={disabled}
        onChange={event => change({ ...value, family: event.target.value as PanelFamily | '' })}>
        <option value="">Let the agent choose</option>
        {PANEL_FAMILIES.map(family => <option value={family.id} key={family.id}>{family.label}</option>)}
      </select>
      {value.family && <p className="ui-help">{PANEL_FAMILIES.find(family => family.id === value.family)?.description}</p>}
    </div>
    {value.family === 'chart' && <div className="workspace-panel-field">
      <label htmlFor={`${id}-variant`}>Chart style</label>
      <select id={`${id}-variant`} value={value.variant} disabled={disabled}
        onChange={event => change({ ...value, variant: event.target.value as ChartVariant })}>
        {CHART_VARIANTS.map(variant => <option key={variant.id} value={variant.id}>{variant.label}</option>)}
      </select>
    </div>}
  </>;
}
