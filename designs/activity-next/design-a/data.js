// Sample data for the Design A prototype. The company, people and numbers are fictional.
window.Proto = { views: {} };
(() => {
const P = window.Proto;
P.you = 'Meera Iyer';
P.colours = { Iris: '#8a4b3c', Atlas: '#51697a', Nova: '#b8860b', Ledger: '#2f7a78', Pixel: '#d4502a', Scout: '#5b5bd6', Echo: '#7a5195', Mint: '#3f7d4e' };

// Everything that waits on a person, already in urgency order inside each group.
P.makeItems = () => [
  { id: 'refunds', group: 'decide', kind: 'group', op: 'Refund order', agent: 'Atlas', where: '#returns', outside: '#returns', age: '5 h', urgent: 'Admin can decide now', unread: true,
    chip: ['wait', 'Waiting for approval'], facts: 'three customers asked in #returns · oldest 5 hours ago',
    lead: 'From Atlas. Each is decided separately; nothing runs until you decide it.',
    notes: [['wait', 'Refunds always ask. Ravi Kumar has not answered for 4 hours, so you can decide as an admin.']],
    subs: [
      { id: 'r1', title: 'Refund order #1049', fields: [['Customer', 'Anita Desai'], ['Amount', '₹2,400'], ['Reason', 'Chair arrived damaged']] },
      { id: 'r2', title: 'Refund order #1052', fields: [['Customer', 'Rohan Mehta'], ['Amount', '₹6,150'], ['Reason', 'Wrong colour sent']] },
      { id: 'r3', title: 'Refund order #1061', fields: [['Customer', 'Kapoor Interiors'], ['Amount', '₹1,980'], ['Reason', 'Cancelled before dispatch']] }] },
  { id: 'discount', group: 'decide', kind: 'approval', title: 'Approve a 12% discount for Sharma Traders', agent: 'Nova', where: '#sales', age: '18 min', unread: true,
    chip: ['wait', 'Waiting for your approval'], facts: 'asked by Kavya Nair in #sales · today 10:36 am',
    card: { label: 'What Nova wants to do', head: 'Zoho Books · Create quote', fields: [['Customer', 'Sharma Traders'], ['Items', '40 × Teak dining chair'], ['Discount', '12%'], ['Total', '₹4,18,000']] },
    notes: [['wait', 'Discounts above 10% need a manager. Waiting for you or Ravi Kumar. If nobody decides by 2:40 pm, an admin can.']],
    help: 'Approval applies once to these exact fields. The first decision counts.',
    steps: [['ok', 'Read Kavya’s request'], ['ok', 'Checked stock: 52 chairs available'], ['ok', 'Prepared the quote'], ['wait', 'Waiting for your approval'], ['next', 'Send the quote to the customer']],
    said: { approve: 'Approved. Nova is sending the quote.', reject: 'Rejected. Nova told Kavya in #sales.', changes: 'Changes requested. Ask Nova for a revised quote in #sales.' },
    talk: { place: '#sales', see: 'Kavya Nair and Nova', msgs: [
      { who: 'Kavya Nair', time: '10:31 am', text: 'Sharma Traders want 40 teak dining chairs for their new showroom. They are asking for a bulk price.' },
      { who: 'Kavya Nair', time: '10:36 am', text: '@Nova send them a quote with 12% off. They ordered 25 chairs in March.' },
      { who: 'Nova', agent: true, target: true, time: '10:37 am', text: 'The quote is ready: 40 chairs, 12% off, ₹4,18,000. A discount above 10% needs a manager, so I have asked for approval before I send it.' }] } },
  { id: 'address', group: 'decide', kind: 'approval', title: 'Update order #1057 delivery address', agent: 'Atlas', where: '#orders', age: '25 min', unread: true,
    chip: ['wait', 'Waiting for your approval'], facts: 'asked by Kavya Nair in #orders · today 10:29 am',
    card: { label: 'What Atlas wants to do', head: 'Zoho Books · Update order', fields: [['Order', '#1057 · Kapoor Interiors'], ['Old address', '14 MG Road, Pune'], ['New address', '8 Baner Road, Pune']] },
    notes: [['wait', 'Changing an order asks for approval. Waiting for you. If nobody decides by 1:54 pm, an admin can.']],
    help: 'Approval applies once to these exact fields. The first decision counts.',
    steps: [['ok', 'Read Kavya’s request'], ['ok', 'Read order #1057'], ['wait', 'Waiting for your approval'], ['next', 'Update the order']],
    said: { approve: 'Approved. Atlas is updating the order.', reject: 'Rejected. Atlas told Kavya in #orders.', changes: 'Changes requested. Ask Atlas for a revised update in #orders.' },
    talk: { place: '#orders', see: 'Kavya Nair and Atlas', msgs: [
      { who: 'Kavya Nair', time: '10:27 am', text: 'Kapoor Interiors moved their site office. Order #1057 should go to the new place.' },
      { who: 'Kavya Nair', time: '10:29 am', text: '@Atlas change the delivery address on #1057 to 8 Baner Road, Pune.' },
      { who: 'Atlas', agent: true, target: true, time: '10:29 am', text: 'I can change it. Changing an order needs an approval first, so I have asked for one.' }] } },
  { id: 'courier', group: 'answer', kind: 'choice', title: 'Which courier for the Bengaluru order?', agent: 'Atlas', where: '#orders', age: '1 h', unread: true,
    chip: ['wait', 'Waiting for your answer'], facts: 'for Ravi Kumar’s request “Ship the Bengaluru order” · #orders',
    quote: 'Two couriers can take the Bengaluru order. Which one should I book?',
    options: [['bluedart', 'BlueDart', '₹840 · arrives Tuesday'], ['delhivery', 'Delhivery', '₹610 · arrives Thursday']],
    help: 'An answer supplies facts. Booking the courier still follows Atlas’s own rules.',
    steps: [['ok', 'Read Ravi’s request'], ['ok', 'Checked two couriers'], ['wait', 'Waiting for your answer'], ['next', 'Book the courier']],
    talk: { place: '#orders', see: 'Ravi Kumar and Atlas', msgs: [
      { who: 'Ravi Kumar', time: '9:52 am', text: '@Atlas ship the Bengaluru order today. Pick whichever courier makes sense.' },
      { who: 'Atlas', agent: true, target: true, quote: true, time: '9:54 am', text: 'Two couriers can take the Bengaluru order. BlueDart is ₹840 and arrives Tuesday. Delhivery is ₹610 and arrives Thursday. Which one should I book?' }] } },
  { id: 'portal', group: 'answer', kind: 'browser', title: 'Sign in to the supplier portal', agent: 'Ledger', where: '#accounts', age: '40 min', unread: true,
    chip: ['wait', 'Waiting in its browser'], facts: 'for your request “Download the October invoice” · #accounts',
    quote: 'The supplier portal asks for a password and a code sent to your phone. Please sign in, then give the browser back to me.',
    help: 'Waiting costs nothing. You have 15 minutes from your last action in the browser.',
    talk: { place: '#accounts', see: 'Ledger', mine: true, msgs: [
      { who: 'You', mine: true, time: '10:12 am', text: '@Ledger download the October invoice from the supplier portal and file it.' },
      { who: 'Ledger', agent: true, target: true, quote: true, time: '10:14 am', text: 'The supplier portal asks for a password and a code sent to your phone. Please sign in, then give the browser back to me.' }] } },
  { id: 'zoho', group: 'fix', kind: 'connection', title: 'Zoho Books needs reconnecting', agent: 'Nova', agents: ['Nova', 'Ledger'], where: 'stops Nova and Ledger', age: '3 h', unread: true,
    chip: ['bad', 'Connection stopped'], facts: 'sign-in expired today 7:50 am',
    card: { label: 'What it is blocking', head: '2 agents cannot read or write in Zoho Books', fields: [['Nova', '2 requests could not complete'], ['Ledger', 'Bank reconciliation at 12:54 pm will fail'], ['Atlas', 'Not affected, it uses another account']] },
    notes: [['bad', 'Reconnecting signs in to Zoho Books again. Nothing is retried by itself; failed requests stay in this list.']] },
  { id: 'iyer', group: 'fix', kind: 'failed', title: 'Draft a quote for Iyer and Sons could not complete', agent: 'Nova', where: '#sales', age: 'yesterday', unread: true,
    chip: ['bad', 'Could not complete'], facts: 'asked by Kavya Nair in #sales · yesterday 4:05 pm · 41 s',
    quote: 'I could not finish the quote: the current price list is not in the Library, so I have no prices for teak tables.',
    notes: [['bad', '<b>How to fix it.</b> Add the current price list to the Library folder Nova can read. Only Kavya Nair can retry, because she asked.']],
    steps: [['ok', 'Read the message'], ['ok', 'Zoho Books: read customer Iyer and Sons', 'Allowed automatically'], ['ok', 'Searched the Library for the price list', 'Allowed automatically'], ['bad', 'Stopped and replied']],
    talk: { place: '#sales', see: 'Kavya Nair and Nova', msgs: [
      { who: 'Kavya Nair', time: 'yesterday 4:05 pm', text: '@Nova draft a quote for Iyer and Sons: 12 teak chairs and 2 tables, usual terms.' },
      { who: 'Nova', agent: true, target: true, quote: true, time: 'yesterday 4:06 pm', text: 'I could not finish the quote: the current price list is not in the Library, so I have no prices for teak tables.' }] } },
  { id: 'gupta', group: 'fix', kind: 'unconfirmed', title: 'Did the payment reminder to Gupta Furniture go out?', agent: 'Ledger', where: '#accounts', age: 'yesterday', unread: true,
    room: true, chip: ['wait', 'Outcome unconfirmed'], facts: 'schedule “Payment reminders” · yesterday 6:54 pm',
    card: { label: 'What Ledger sent', head: 'Gmail · Send email', fields: [['To', 'accounts@guptafurniture.example'], ['Subject', 'Reminder: invoice INV-2291 is 14 days overdue'], ['Checked', 'Looked for it: Gmail didn’t answer']] },
    notes: [['wait', 'Gmail didn’t confirm this action. Cuckoo has not tried it again. Gmail writes are paused until someone answers, so it is never done twice.']] },
  { id: 'late', group: 'fix', kind: 'task', task: 't-gupta', title: 'Late: chase Gupta Furniture for the October invoice', agent: 'Ledger', where: '#accounts', age: '3 h late', unread: true,
    chip: ['bad', 'Late'], facts: 'task for you · due today 7:54 am · next check today 2:00 pm',
    quote: 'I emailed Gupta Furniture twice and have no reply. I can call their accounts desk if you give me the number.',
    notes: [['bad', 'A late task stays here until it is done or cancelled.']],
    steps: [['ok', 'Emailed Gupta Furniture', 'Wed 4:10 pm'], ['ok', 'Emailed again', 'Fri 10:02 am'], ['wait', 'No reply yet']],
    talk: { place: '#accounts', see: 'Ledger', mine: true, msgs: [
      { who: 'You', mine: true, time: 'Wed 4:02 pm', text: '@Ledger chase Gupta Furniture for the October invoice. I need it by Saturday morning.' },
      { who: 'Ledger', agent: true, target: true, quote: true, time: 'Fri 10:03 am', text: 'I emailed Gupta Furniture twice and have no reply. I can call their accounts desk if you give me the number.' }] } },
];
P.arrivals = [
  { id: 'paylink', group: 'decide', kind: 'approval', title: 'Send a payment link to Kapoor Interiors', agent: 'Atlas', where: '#orders', age: 'just now', unread: true, fresh: true,
    chip: ['wait', 'Waiting for your approval'], facts: 'asked by Ravi Kumar in #orders · just now',
    card: { label: 'What Atlas wants to do', head: 'Razorpay · Create payment link', fields: [['Customer', 'Kapoor Interiors'], ['Amount', '₹86,400'], ['For', 'Order #1057'], ['Expires', 'in 7 days']] },
    notes: [['wait', 'Payment links above ₹50,000 ask for approval. Waiting for you. If nobody decides by 2:54 pm, an admin can.']],
    help: 'Approval applies once to these exact fields. The first decision counts.',
    steps: [['ok', 'Read Ravi’s request'], ['ok', 'Read order #1057'], ['wait', 'Waiting for your approval'], ['next', 'Send the link by email']],
    said: { approve: 'Approved. Atlas is sending the payment link.', reject: 'Rejected. Atlas told Ravi in #orders.', changes: 'Changes requested. Ask Atlas for a revised link in #orders.' },
    talk: { place: '#orders', see: 'Ravi Kumar and Atlas', msgs: [
      { who: 'Ravi Kumar', time: 'just now', text: '@Atlas send Kapoor Interiors a payment link for order #1057.' },
      { who: 'Atlas', agent: true, target: true, time: 'just now', text: 'The link is ready for ₹86,400. Links above ₹50,000 need an approval, so I have asked for one.' }] } },
  { id: 'tone', group: 'answer', kind: 'choice', title: 'Formal or friendly tone for the Diwali posts?', agent: 'Pixel', where: '#marketing', age: 'just now', unread: true, fresh: true,
    chip: ['wait', 'Waiting for your answer'], facts: 'for your task “Prepare Diwali offer posts” · #marketing',
    quote: 'Last year’s posts were formal. This year’s catalogue copy is friendlier. Which tone should the Diwali posts use?',
    options: [['formal', 'Formal', 'like last year'], ['friendly', 'Friendly', 'like the new catalogue']],
    help: 'An answer supplies facts. Posting still follows Pixel’s own rules.',
    talk: { place: '#marketing', see: 'Pixel', mine: true, msgs: [
      { who: 'You', mine: true, time: 'Thu 3:20 pm', text: '@Pixel prepare the Diwali offer posts for next week.' },
      { who: 'Pixel', agent: true, target: true, quote: true, time: 'just now', text: 'Last year’s posts were formal. This year’s catalogue copy is friendlier. Which tone should the Diwali posts use?' }] } },
];

P.makeAgents = () => [
  { name: 'Nova', role: 'Sales follow-up', next: '3:54 pm', today: '9 requests · 2 failed', doing: [{ id: 'd1', title: 'Quote for Sharma Traders', meta: 'Kavya Nair · #sales · waiting for approval · 18 min' }],
    coming: [['3:54 pm', 'Check if Sharma Traders replied', 'Follow-up'], ['Mon 10:54', 'Monday sales summary', 'Every week']], month: '60 of 71 tasks done · needed a person 24 times · goal: 31 of 50 quotes this month' },
  { name: 'Ledger', role: 'Accounts helper', next: '12:54 pm', today: '6 requests · 1 unconfirmed', doing: [{ id: 'd2', title: 'Download the October invoice', meta: 'You · #accounts · waiting in its browser · 40 min' }],
    coming: [['12:54 pm', 'Bank reconciliation', 'Every day · will fail until Zoho Books is reconnected'], ['—', 'Month-end report', 'Paused']], month: '51 of 54 tasks done · needed a person 9 times' },
  { name: 'Atlas', role: 'Orders assistant', next: 'On new orders', today: '14 requests · 0 failed',
    doing: [{ id: 'd3', title: 'Update the Kapoor order delivery address', meta: 'Ravi Kumar · direct chat · working · 2 min', stop: true }, { id: 'd4', title: 'Ship the Bengaluru order', meta: 'Ravi Kumar · #orders · waiting for an answer · 1 h', stop: true }],
    coming: [['Always on', 'New order in Zoho Books', 'Trigger']], month: '88 of 96 tasks done · needed a person 31 times' },
  { name: 'Iris', role: 'Customer support', next: '11:24 am', today: '63 requests · 0 failed', doing: [{ id: 'd5', title: 'Reply to 6 customer emails', meta: 'Schedule · shared inbox · working · 4 of 6 · 11 min', stop: true }],
    coming: [['11:24 am', 'Reply to the shared inbox', 'Every hour']], month: '204 of 212 tasks done · needed a person 11 times · goal at risk: first reply within 2 hours is at 81%, target 95%' },
  { name: 'Pixel', role: 'Marketing posts', next: '4:54 pm', today: '3 requests · 0 failed', doing: [],
    coming: [['4:54 pm', 'Post new arrivals', 'Every day'], ['Mon 10:54', 'Prepare Diwali offer posts', 'Task due']], month: '31 of 33 tasks done · needed a person 4 times' },
  { name: 'Scout', role: 'Stock checker', next: 'Skipped while paused', today: 'No requests', paused: 'Paused by Ravi Kumar on 28 Oct', doing: [],
    coming: [['Skipped', 'Morning stock check', 'Every day · runs again after Resume']], month: '12 of 12 tasks done before the pause' },
  { name: 'Echo', role: 'Hiring helper', owner: 'Ravi Kumar', next: 'Mon 9:00 am', today: '4 requests · 0 failed', doing: [],
    coming: [['Mon 9:00', 'Weekly applicants summary', 'Every week']], month: '22 of 23 tasks done · needed a person 6 times' },
  { name: 'Mint', role: 'Payroll helper', owner: 'Ananya Rao', next: '30 Nov', today: '1 request · 0 failed', doing: [{ id: 'd6', title: 'Check November attendance sheet', meta: 'Ananya Rao · direct chat · working · 6 min' }],
    coming: [['30 Nov', 'Prepare payroll run', 'Every month']], month: '9 of 9 tasks done · needed a person 9 times' },
];

P.makeTasks = () => [
  { id: 't-iyer', status: 'Needs fixing', tone: 'bad', title: 'Send Iyer and Sons a quote once the price list is back', agent: 'Nova', meta: 'Kavya Nair · #sales · 3 steps · couldn’t continue', item: 'iyer' },
  { id: 't-gupta', status: 'Late', tone: 'bad', title: 'Chase Gupta Furniture for the October invoice', agent: 'Ledger', meta: 'You · #accounts · due today 7:54 am', item: 'late' },
  { id: 't-sharma', status: 'Waiting', tone: 'wait', title: 'Quote for Sharma Traders, 40 chairs', agent: 'Nova', meta: 'Kavya Nair · #sales · waiting for an approval · 4 steps', item: 'discount' },
  { id: 't-blr', status: 'Waiting', tone: 'wait', title: 'Ship the Bengaluru order', agent: 'Atlas', meta: 'Ravi Kumar · #orders · waiting for an answer', item: 'courier' },
  { id: 't-warranty', status: 'Working', tone: 'work', title: 'Reply to this week’s warranty emails', agent: 'Iris', meta: 'You · direct chat · 6 steps' },
  { id: 't-diwali', status: 'Open', tone: '', title: 'Prepare Diwali offer posts', agent: 'Pixel', meta: 'Kavya Nair · #marketing · due Mon 10:54 am' },
  { id: 't-follow', status: 'Follow-up set', tone: '', title: 'Check if Sharma Traders replied', agent: 'Nova', meta: 'Kavya Nair · #sales · next check today 3:54 pm' },
];

P.makePlanned = () => [
  { id: 'p1', kind: 'schedule', day: 'Today', time: '11:24 am', agent: 'Iris', title: 'Reply to the shared inbox', cadence: 'Every hour', by: 'Ravi Kumar' },
  { id: 'p2', kind: 'schedule', day: 'Today', time: '12:54 pm', agent: 'Ledger', title: 'Bank reconciliation', cadence: 'Every day', by: 'you', warn: 'Will fail until Zoho Books is reconnected' },
  { id: 'p3', kind: 'follow_up', day: 'Today', time: '3:54 pm', agent: 'Nova', title: 'Check if Sharma Traders replied', cadence: 'The agent checks back on this task' },
  { id: 'p4', kind: 'schedule', day: 'Today', time: '4:54 pm', agent: 'Pixel', title: 'Post new arrivals', cadence: 'Every day', by: 'you' },
  { id: 'p5', kind: 'schedule', day: 'Tomorrow', time: '12:54 am', agent: 'Scout', title: 'Morning stock check', cadence: 'Every day', by: 'Ravi Kumar', warn: 'Skipped while Scout is paused' },
  { id: 'p6', kind: 'schedule', day: 'Mon, 23 Nov', time: '9:00 am', agent: 'Echo', title: 'Weekly applicants summary', cadence: 'Every week', by: 'Ravi Kumar' },
  { id: 'p7', kind: 'schedule', day: 'Mon, 23 Nov', time: '10:54 am', agent: 'Nova', title: 'Monday sales summary', cadence: 'Every week', by: 'you' },
  { id: 'p8', kind: 'due', day: 'Mon, 23 Nov', time: '10:54 am', agent: 'Pixel', title: 'Prepare Diwali offer posts', cadence: 'This task is due', task: 't-diwali' },
  { id: 'p9', kind: 'schedule', paused: true, agent: 'Ledger', title: 'Month-end report', cadence: 'Every month', by: 'you' },
  { id: 'p10', kind: 'trigger', agent: 'Atlas', title: 'New order in Zoho Books', cadence: 'Zoho Books · order created', by: 'Ravi Kumar' },
  { id: 'p11', kind: 'trigger', paused: true, agent: 'Pixel', title: 'New product in the catalogue', cadence: 'Shop · product created', by: 'you' },
];

const steps = (...list) => list.map(text => Array.isArray(text) ? text : ['ok', text]);
P.makeHistory = () => [
  { id: 'h1', day: 'Today', time: '10:52 am', agent: 'Atlas', type: 'chat', quoted: true, title: 'Update the Kapoor order delivery address', meta: 'Ravi Kumar · direct chat',
    lines: ['Zoho Books: read order #1042 · allowed automatically', 'Zoho Books: update order #1042 · approved by you'], status: ['work', 'Working'],
    detail: { facts: 'asked by Ravi Kumar in a direct chat · today 10:52 am', steps: steps('Read the message', ['ok', 'Zoho Books: read order #1042', 'Allowed automatically'], ['ok', 'Zoho Books: update order #1042', 'Approved by you'], ['wait', 'Checking the courier label']) } },
  { id: 'h2', day: 'Today', time: '10:36 am', agent: 'Nova', type: 'chat', quoted: true, title: 'Quote for Sharma Traders, 40 chairs', meta: 'Kavya Nair · #sales', status: ['wait', 'Waiting for approval'], item: 'discount' },
  { id: 'h3', day: 'Today', time: '9:24 am', agent: 'Iris', type: 'schedule', title: 'Reply to 6 customer emails', meta: 'Schedule · shared inbox', status: ['work', 'Working'] },
  { id: 'h4', day: 'Today', time: '8:54 am', agent: 'Ledger', type: 'panel', title: 'Updated panel: Sales today', meta: 'Panel update · updated 12 times today', status: ['', 'Show all'], fold: ['8:54 am', '7:54 am', '6:54 am', '5:54 am'] },
  { id: 'h5', day: 'Today', time: '6:54 am', agent: 'Atlas', type: 'chat', quoted: true, light: true, title: 'how many orders today?', meta: 'Ravi Kumar · #orders', status: ['done', 'Done'],
    detail: { facts: 'asked by Ravi Kumar in #orders · today 6:54 am · 3 s', quote: 'There are 14 orders so far today, worth ₹3,92,600. Two are waiting for dispatch.', steps: steps('Read the message', ['ok', 'Zoho Books: list today’s orders', 'Allowed automatically'], 'Replied') } },
  { id: 'h6', day: 'Yesterday', time: '8:54 pm', agent: 'Pixel', type: 'trigger', title: 'Post new arrivals', meta: 'Trigger · 3 new products in the catalogue', status: ['done', 'Done'],
    detail: { facts: 'started by the trigger “New product in the catalogue” · yesterday 8:54 pm · 26 s', steps: steps('Read 3 new products', ['ok', 'Instagram: create post', 'Allowed automatically'], 'Posted a summary in #marketing') } },
  { id: 'h7', day: 'Yesterday', time: '6:54 pm', agent: 'Ledger', type: 'schedule', title: 'Payment reminder to Gupta Furniture', meta: 'Schedule · Payment reminders', lines: ['Gmail: send email · not confirmed'], status: ['wait', 'Unconfirmed'], item: 'gupta' },
  { id: 'h8', day: 'Yesterday', time: '4:05 pm', agent: 'Nova', type: 'chat', quoted: true, title: 'Draft a quote for Iyer and Sons', meta: 'Kavya Nair · #sales · price list missing', status: ['bad', 'Could not complete'], item: 'iyer' },
  { id: 'h9', day: 'Yesterday', time: '11:20 am', agent: 'Iris', type: 'schedule', title: 'Reply to the shared inbox', meta: 'Schedule · 9 emails answered', status: ['done', 'Done'] },
  { id: 'h10', day: 'Thu, 19 Nov', time: '5:12 pm', agent: 'Atlas', type: 'chat', quoted: true, title: 'Cancel order #1038 and tell the customer', meta: 'Ravi Kumar · #orders', lines: ['Zoho Books: cancel order #1038 · approved by Ravi Kumar'], status: ['done', 'Done'] },
  { id: 'h11', day: 'Thu, 19 Nov', time: '2:40 pm', agent: 'Nova', type: 'schedule', title: 'Weekly review', meta: 'Schedule · written for you', status: ['done', 'Done'] },
];
P.olderHistory = () => [
  { id: 'h12', day: 'Wed, 18 Nov', time: '4:10 pm', agent: 'Ledger', type: 'chat', quoted: true, title: 'Email Gupta Furniture about the October invoice', meta: 'You · #accounts', lines: ['Gmail: send email · allowed automatically'], status: ['done', 'Done'] },
  { id: 'h13', day: 'Wed, 18 Nov', time: '11:02 am', agent: 'Scout', type: 'chat', quoted: true, title: 'Is teak table stock below 10?', meta: 'Kavya Nair · #sales', status: ['', 'Stopped'] },
  { id: 'h14', day: 'Tue, 17 Nov', time: '9:30 am', agent: 'Iris', type: 'schedule', title: 'Reply to the shared inbox', meta: 'Schedule · 14 emails answered', status: ['done', 'Done'] },
];

P.insights = {
  perDay: [['Sun', 28], ['Mon', 31], ['Tue', 29], ['Wed', 37], ['Thu', 34], ['Fri', 22], ['Sat', 33]],
  byAgent: [['Iris', 63], ['Nova', 48], ['Atlas', 41], ['Ledger', 32], ['Pixel', 21], ['Scout', 9]],
  table: [
    ['Iris', '204 of 212', 3, '98 of 102', 11, 38, 'Goal at risk: first reply within 2 hours'],
    ['Atlas', '88 of 96', 4, '38 of 44', 31, 74, 'Asks you most often. See the suggestion above'],
    ['Nova', '60 of 71', 7, '25 of 30', 24, 96, 'Fails most: 7 of 71, mostly a missing price list'],
    ['Ledger', '51 of 54', 1, '24 of 26', 9, 21, ''],
    ['Pixel', '31 of 33', 0, '15 of 16', 4, 9, ''],
    ['Scout', '12 of 12', 0, '6 of 6', 0, 0, 'Paused since 28 Oct'],
  ],
  goals: [['Nova', 'Quotes sent', 31, 50, 'quotes this month', ''], ['Iris', 'First reply within 2 hours', 81, 95, '% this week', 'Monday backlog after the long weekend']],
};
})();
