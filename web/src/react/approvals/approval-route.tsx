export type Approver = { id: string; name: string; revision: number | null };
export type Route = { approver?: string | null; approver_revision?: number | null; approval_chat?: string | null };
export function ApprovalRoute({ label, route, people, channels, disabled, change }: { label: string; route: Route; people: Approver[]; channels: { id: string; name: string }[]; disabled?: boolean; change: (route: Route) => void }) {
  return <div className="approval-route">
    <label className="ui-field">Approver<select aria-label={`${label} approver`} value={route.approver ?? ''} disabled={disabled} onChange={e => {
      const person = people.find(p => p.id === e.target.value);
      change({ ...route, approver: person?.id, approver_revision: person?.revision });
    }}><option value="">Default authorized admin</option>{people.map(person => <option key={person.id} value={person.id}>{person.name}</option>)}</select></label>
    <label className="ui-field">Deliver request in<select aria-label={`${label} approval channel`} value={route.approval_chat ?? ''} disabled={disabled} onChange={e => change({ ...route, approval_chat: e.target.value || undefined })}>
      <option value="">Work conversation</option>{channels.map(channel => <option key={channel.id} value={channel.id}>{channel.name}</option>)}
    </select></label>
    <p className="ui-help">The approver needs explicit action authority and access to the source work. The approval channel must have a suitable audience.</p>
  </div>;
}
