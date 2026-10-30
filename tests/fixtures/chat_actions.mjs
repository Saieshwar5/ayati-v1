// Message actions as a person uses them: hover the message, open ⋯, choose.
const latestReply = page => page.locator('article.chat-message:has(.chat-reply-progress)').last();

export async function openMessageMenu(message) {
  await message.hover();
  await message.getByRole('button', { name: 'More actions', exact: true }).click();
}

/** Cancels a running request from its reply's menu (the requester only sees it). */
export async function cancelRequest(page, message = latestReply(page)) {
  await openMessageMenu(message);
  await message.getByRole('menuitem', { name: 'Cancel request', exact: true }).click();
}

/** Whether the reply's menu offers Cancel request; leaves the menu closed. */
export async function offersCancel(message) {
  await openMessageMenu(message);
  const offered = await message.getByRole('menuitem', { name: 'Cancel request', exact: true }).count() > 0;
  await message.getByRole('button', { name: 'More actions', exact: true }).click();
  return offered;
}
