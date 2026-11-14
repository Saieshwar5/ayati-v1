/** How a message reached Cuckoo when it was not typed in Chat (E3): the door's
 * own words, so a person's direct conversation shows which message came by
 * WhatsApp or email. Chat messages have no door. */
const names: Record<string, string> = { whatsapp: 'WhatsApp', email: 'email' };
export const doorLabel = (door?: string | null) => (door ? `via ${names[door] ?? door}` : null);
