window.APP = {
  collection: 'requests',
  add: 'Ask for leave',
  listTitle: 'Requests',
  empty: 'No requests yet.',
  fields: [
    { name: 'start', label: 'First day', type: 'date', required: true },
    { name: 'days', label: 'Days', type: 'number', required: true },
    { name: 'reason', label: 'Reason', type: 'text' },
  ],
  decide: { field: 'status', start: 'waiting', roles: ['approver'], choices: [{ label: 'Approve', value: 'approved' }, { label: 'Decline', value: 'declined' }] },
};
