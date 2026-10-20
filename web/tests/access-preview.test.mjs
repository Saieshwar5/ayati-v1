import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  accept, cancelInvitation, changeMember, createBusiness, emptyState, invite, membership,
} from '../src/access-preview-state.ts';

function fixture() {
  const state = emptyState();
  const business = createBusiness(state, 'owner@example.com', 'Example business');
  return { state, business };
}

test('invitation acceptance checks recipient, expiry and cancellation before membership', () => {
  const { state, business } = fixture();
  const invitation = invite(state, business, 'owner@example.com', 'employee@example.com', 'Member', 100);
  assert.throws(() => accept(state, invitation.id, 'other@example.com', 101), /Switch/);
  assert.throws(() => accept(state, invitation.id, invitation.email, invitation.expires), /expired/);
  assert.equal(business.members.length, 1);
  cancelInvitation(business, 'owner@example.com', invitation);
  assert.throws(() => accept(state, invitation.id, invitation.email, 101), /no longer/);
});

test('accepting twice does not duplicate membership; removed member cannot reuse accepted link', () => {
  const { state, business } = fixture();
  const invitation = invite(state, business, 'owner@example.com', 'employee@example.com', 'Member');
  accept(state, invitation.id, invitation.email);
  accept(state, invitation.id, invitation.email);
  assert.equal(business.members.length, 2);
  changeMember(business, 'owner@example.com', invitation.email, null);
  assert.throws(() => accept(state, invitation.id, invitation.email), /removed/);
});

test('member and admin role boundaries preserve owner and unrelated workspace', () => {
  const { state, business } = fixture();
  const other = createBusiness(state, 'other@example.com', 'Other business');
  business.members.push({ email: 'member@example.com', role: 'Member' }, { email: 'admin@example.com', role: 'Admin' });
  assert.throws(() => invite(state, business, 'member@example.com', 'new@example.com', 'Member'), /owners and admins/);
  assert.throws(() => invite(state, business, 'admin@example.com', 'new@example.com', 'Admin'), /Only the owner/);
  assert.throws(() => changeMember(business, 'owner@example.com', 'owner@example.com', null), /ownership-transfer/);
  assert.throws(() => changeMember(business, 'admin@example.com', 'member@example.com', 'Admin'), /Only the owner/);
  assert.throws(() => invite(state, other, 'owner@example.com', 'new@example.com', 'Member'), /owners and admins/);
  assert.equal(membership(other, 'owner@example.com'), undefined);
});

test('duplicate pending invites are refused and reissue invalidates old invitation', () => {
  const { state, business } = fixture();
  const old = invite(state, business, 'owner@example.com', 'employee@example.com', 'Member');
  assert.throws(() => invite(state, business, 'owner@example.com', old.email, 'Member'), /already pending/);
  cancelInvitation(business, 'owner@example.com', old);
  const fresh = invite(state, business, 'owner@example.com', old.email, 'Member');
  assert.notEqual(old.id, fresh.id);
  assert.throws(() => accept(state, old.id, old.email), /no longer/);
  accept(state, fresh.id, fresh.email);
});
