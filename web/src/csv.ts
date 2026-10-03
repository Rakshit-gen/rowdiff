// Reading just enough of a CSV in the browser to offer its columns before
// anything is uploaded.

const CANDIDATES = [",", ";", "\t", "|"] as const;

/** Split the first record of `text`, honoring quotes and "" escapes. */
export function parseHeader(text: string, delimiter: string): string[] {
  const s = text.charCodeAt(0) === 0xfeff ? text.slice(1) : text;
  const fields: string[] = [];
  let field = "";
  let quoted = false;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (quoted) {
      if (c === '"' && s[i + 1] === '"') {
        field += '"';
        i++;
      } else if (c === '"') {
        quoted = false;
      } else {
        field += c;
      }
    } else if (c === '"' && field === "") {
      quoted = true;
    } else if (c === delimiter) {
      fields.push(field);
      field = "";
    } else if (c === "\n" || c === "\r") {
      break;
    } else {
      field += c;
    }
  }
  fields.push(field);
  return fields;
}

/** The delimiter that splits the header into the most fields. Commas win ties. */
export function sniffDelimiter(text: string): string {
  let best: string = ",";
  let most = 0;
  for (const d of CANDIDATES) {
    const n = parseHeader(text, d).length;
    if (n > most) {
      best = d;
      most = n;
    }
  }
  return best;
}

export interface FileHeader {
  columns: string[];
  delimiter: string;
}

/** Read the header of a picked file from its first 64 KB. */
export async function readHeader(file: File): Promise<FileHeader> {
  const text = await file.slice(0, 64 * 1024).text();
  const delimiter = sniffDelimiter(text);
  return { columns: parseHeader(text, delimiter), delimiter };
}

/** A likely key among columns both files share, or null when nothing stands out. */
export function guessKey(columns: string[]): string | null {
  const lower = columns.map((c) => c.toLowerCase());
  for (const exact of ["id", "sku", "key", "uuid", "code"]) {
    const i = lower.indexOf(exact);
    if (i >= 0) return columns[i]!;
  }
  const i = lower.findIndex((c) => c.endsWith("_id") || c.endsWith(" id"));
  return i >= 0 ? columns[i]! : null;
}
