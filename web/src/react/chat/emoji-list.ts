/** Common emoji with search words, most useful at work first. Phones also offer the system picker. */
const LIST = `
👍 thumbs up yes ok like agree
✅ done check complete yes
👀 eyes looking seen
🙏 thanks please pray
🎉 party celebrate congratulations
❤️ heart love
😂 laugh joy funny
🔥 fire hot great
👏 clap applause well done
💯 hundred perfect
🤔 thinking hmm
😮 wow surprised
😊 smile happy blush
🙂 slight smile
😄 grin happy
😅 sweat smile phew
😁 beaming grin
😆 laughing
🤣 rolling laughing
😉 wink
😍 heart eyes love
🥰 smiling hearts
😎 cool sunglasses
🤩 star struck amazing
😇 innocent halo
🙃 upside down
😌 relieved
😴 sleeping tired
🥱 yawn tired
😐 neutral
😑 expressionless
😬 grimace awkward
🙄 eye roll
😏 smirk
😒 unamused
😔 pensive sad
😢 cry sad tear
😭 sobbing
😤 frustrated
😠 angry
😡 rage angry
🤯 mind blown
😳 flushed embarrassed
🥺 pleading
😱 scream shocked
🤗 hug
🤝 handshake deal agree
👌 ok perfect
✌️ peace victory
🤞 fingers crossed luck
👋 wave hello bye
🙌 raised hands hooray
💪 strong muscle
👎 thumbs down no dislike
👉 point right
👆 point up
☝️ index up one
✋ raised hand stop
🫡 salute
🤷 shrug dont know
🙋 raise hand question
💡 idea light bulb
⭐ star favourite
🌟 glowing star
✨ sparkles new
⚡ lightning fast
☕ coffee tea break
🍵 tea
🍕 pizza food
🍰 cake
🎂 birthday cake
🎁 gift present
🏆 trophy win
🥇 first medal gold
🎯 target goal
🚀 rocket launch
📈 chart up growth
📉 chart down decline
📊 bar chart report
💰 money bag
💵 cash money
💳 card payment
🧾 receipt invoice bill
📦 package box parcel order
🚚 truck delivery shipping
🏭 factory
🏪 shop store
🏢 office building
🏠 home house
🛒 cart shopping
🏷️ label tag price
📅 calendar date
🗓️ calendar schedule
⏰ alarm clock time
⏳ hourglass waiting
⌛ hourglass done
🕒 clock three
📌 pin important
📍 location place
📎 paperclip attachment
✏️ pencil edit
📝 memo note
📄 document page
📁 folder
📋 clipboard list
📧 email mail
📨 incoming mail
📞 phone call
📱 mobile phone
💻 laptop computer
🖨️ printer
🔒 lock private secure
🔑 key access
🔍 search find
⚙️ settings gear
🔧 wrench fix
🛠️ tools repair
🧰 toolbox
🔔 bell notification
🔕 mute bell
📣 announcement
💬 speech chat
🗨️ comment
❓ question
❗ exclamation important
⚠️ warning caution
❌ cross no wrong cancel
⛔ no entry stop
🚫 prohibited
✔️ check mark
☑️ ballot check
➕ plus add
➖ minus remove
🔄 repeat refresh
↩️ back reply return
➡️ right arrow next
⬆️ up arrow
⬇️ down arrow
🆗 ok button
🆕 new
🔴 red circle
🟠 orange circle
🟡 yellow circle
🟢 green circle
🔵 blue circle
⚪ white circle
⚫ black circle
🌞 sun sunny
🌧️ rain
❄️ snow cold
🌈 rainbow
🌱 seedling growth
🌸 flower blossom
🍀 clover luck
🐞 bug ladybird
🐢 turtle slow
🤖 robot bot agent
👤 person
👥 people team
🧑‍💻 developer technologist
`;

export type Emoji = { emoji: string; words: string };

export const EMOJI: Emoji[] = LIST.trim().split('\n').map(line => {
  const space = line.indexOf(' ');
  return { emoji: line.slice(0, space), words: line.slice(space + 1) };
});

export function searchEmoji(query: string) {
  const words = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  // Each typed word must start one of the emoji's words ("truck" is not "struck").
  return words.length ? EMOJI.filter(item => words.every(word => item.words.split(' ').some(name => name.startsWith(word)))) : EMOJI;
}
