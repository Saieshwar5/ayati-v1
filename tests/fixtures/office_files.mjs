// Small real Word files for journeys: an uncompressed ("stored") zip of the parts a
// .docx needs. Headings use Word's Heading1 style.
import { crc32 } from 'node:zlib';

function zip(parts) {
  const local = [], central = [];
  let offset = 0;
  for (const [name, text] of parts) {
    const data = Buffer.from(text), file = Buffer.from(name), crc = crc32(data);
    const header = Buffer.alloc(30);
    header.writeUInt32LE(0x04034b50, 0); header.writeUInt16LE(20, 4);
    header.writeUInt32LE(crc, 14); header.writeUInt32LE(data.length, 18); header.writeUInt32LE(data.length, 22);
    header.writeUInt16LE(file.length, 26);
    const entry = Buffer.alloc(46);
    entry.writeUInt32LE(0x02014b50, 0); entry.writeUInt16LE(20, 4); entry.writeUInt16LE(20, 6);
    entry.writeUInt32LE(crc, 16); entry.writeUInt32LE(data.length, 20); entry.writeUInt32LE(data.length, 24);
    entry.writeUInt16LE(file.length, 28); entry.writeUInt32LE(offset, 42);
    local.push(header, file, data); central.push(entry, file);
    offset += header.length + file.length + data.length;
  }
  const directory = Buffer.concat(central), end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0); end.writeUInt16LE(parts.length, 8); end.writeUInt16LE(parts.length, 10);
  end.writeUInt32LE(directory.length, 12); end.writeUInt32LE(offset, 16);
  return Buffer.concat([...local, directory, end]);
}

const escape = text => text.replace(/&/g, '&amp;').replace(/</g, '&lt;');
/** A .docx from [heading, paragraph] pairs. */
export function wordFile(sections) {
  const body = sections.map(([heading, text]) =>
    `<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>${escape(heading)}</w:t></w:r></w:p><w:p><w:r><w:t>${escape(text)}</w:t></w:r></w:p>`).join('');
  return zip([
    ['[Content_Types].xml', '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>'],
    ['word/document.xml', `<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>${body}</w:body></w:document>`],
  ]);
}
