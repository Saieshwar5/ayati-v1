window.APP = {
  collection: 'claims',
  add: 'Claim it',
  listTitle: 'Claims',
  empty: 'No claims yet.',
  fields: [
    { name: 'spent_on', label: 'Spent on', type: 'date', required: true },
    { name: 'amount', label: 'Amount', type: 'number', required: true },
    { name: 'what', label: 'What for', type: 'text', required: true },
  ],
  decide: { field: 'status', start: 'waiting', roles: ['approver'], choices: [{ label: 'Approve', value: 'approved' }, { label: 'Decline', value: 'declined' }] },
};
