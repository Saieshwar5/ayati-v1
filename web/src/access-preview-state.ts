// Browser-only fixtures. Never use these identities or roles to authorize API requests.
export type Role = 'Owner' | 'Admin' | 'Member';
export type Member = { email: string; role: Role };
export type Business = { id: string; name: string; members: Member[] };
export type Invitation = {
  id: string; workspace: string; email: string; role: 'Admin' | 'Member';
  expires: number; status: 'pending' | 'accepted' | 'cancelled';
};
export type PreviewState = { businesses: Business[]; invitations: Invitation[] };
export const emptyState = (): PreviewState => ({ businesses: [], invitations: [] });
export const normalizeEmail = (value: string): string => value.trim().toLowerCase();

export function emailAddress(value: string): string {
  const email = normalizeEmail(value);
  if (email.length > 254 || !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
    throw new Error('Enter a valid email address.');
  }
  return email;
}

export function membership(business: Business, email: string): Member | undefined {
  return business.members.find(member => member.email === email);
}

export function createBusiness(state: PreviewState, email: string, name: string): Business {
  const clean = name.trim();
  if (!clean || clean.length > 80) throw new Error('Use a workspace name between 1 and 80 characters.');
  const business: Business = {
    id: crypto.randomUUID(), name: clean, members: [{ email: emailAddress(email), role: 'Owner' }],
  };
  state.businesses.push(business);
  return business;
}

function requireManager(business: Business, actor: string): Role {
  const role = membership(business, actor)?.role;
  if (role !== 'Owner' && role !== 'Admin') throw new Error('Only owners and admins can manage people.');
  return role;
}

export function invite(state: PreviewState, business: Business, actor: string, address: string,
  role: 'Admin' | 'Member', now = Date.now()): Invitation {
  const manager = requireManager(business, actor);
  if (!['Admin', 'Member'].includes(role) || (role === 'Admin' && manager !== 'Owner')) {
    throw new Error('Only the owner can appoint an admin.');
  }
  const email = emailAddress(address);
  if (membership(business, email)) throw new Error('This person is already a member.');
  if (state.invitations.some(item => item.workspace === business.id && item.email === email
    && item.status === 'pending' && item.expires > now)) {
    throw new Error('An invitation is already pending. Resend it from People.');
  }
  const invitation: Invitation = { id: crypto.randomUUID(), workspace: business.id, email, role,
    expires: now + 7 * 24 * 60 * 60 * 1000, status: 'pending' };
  state.invitations.push(invitation);
  return invitation;
}

export function accept(state: PreviewState, id: string, email: string, now = Date.now()): Business {
  const invitation = state.invitations.find(item => item.id === id);
  if (!invitation) throw new Error('This demo invitation was not found. Open it in the same browser.');
  if (invitation.email !== email) throw new Error(`Switch to ${invitation.email} to accept this invitation.`);
  const business = state.businesses.find(item => item.id === invitation.workspace);
  if (!business || invitation.status === 'cancelled') throw new Error('This invitation is no longer available.');
  if (invitation.status === 'accepted') {
    if (membership(business, email)) return business;
    throw new Error('Your access was removed. Ask the owner for a new invitation.');
  }
  if (invitation.expires <= now) throw new Error('This invitation has expired. Ask the owner to resend it.');
  if (!membership(business, email)) business.members.push({ email, role: invitation.role });
  invitation.status = 'accepted';
  return business;
}

export function cancelInvitation(business: Business, actor: string, invitation: Invitation): void {
  requireManager(business, actor);
  if (invitation.workspace !== business.id) throw new Error('Invitation belongs to another workspace.');
  if (invitation.status !== 'pending') throw new Error('This invitation is no longer pending.');
  invitation.status = 'cancelled';
}

export function changeMember(business: Business, actor: string, email: string, role: Role | null): void {
  const manager = requireManager(business, actor);
  const member = membership(business, email);
  if (!member) throw new Error('This person is no longer a member.');
  if (member.role === 'Owner' || role === 'Owner') {
    throw new Error('Owner changes require a separate ownership-transfer flow.');
  }
  if (manager === 'Admin' && (member.role === 'Admin' || role === 'Admin')) {
    throw new Error('Only the owner can manage admins.');
  }
  if (role) member.role = role;
  else business.members = business.members.filter(item => item.email !== email);
}
