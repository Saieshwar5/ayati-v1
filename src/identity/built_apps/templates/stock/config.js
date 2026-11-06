window.APP = {
  collection: 'counts',
  add: 'Record count',
  listTitle: 'Counts',
  empty: 'Nothing counted yet.',
  fields: [
    { name: 'item', label: 'Item', type: 'text', required: true },
    { name: 'quantity', label: 'Quantity', type: 'number', required: true },
    { name: 'location', label: 'Where', type: 'text' },
  ],
  decide: null,
};
