// 「EXIF」タブの一覧（折りたたみ・コピー・マップは #16 で旧版と同じにする）。

import type { ExifInfo } from "./types";

/** EXIF の一覧を出す。 */
export function showExif(page: HTMLElement, exif: ExifInfo) {
  page.replaceChildren();
  if (exif.makerNote) {
    const note = document.createElement("p");
    note.textContent = `MakerNote の形式: ${exif.makerNote}`;
    page.append(note);
  }
  let group = "";
  let table: HTMLTableElement | null = null;
  for (const entry of exif.entries) {
    if (entry.group !== group || !table) {
      group = entry.group;
      const heading = document.createElement("h2");
      heading.textContent = group;
      table = document.createElement("table");
      page.append(heading, table);
    }
    const row = table.insertRow();
    const name = document.createElement("th");
    name.textContent = entry.label;
    const value = row.insertCell();
    value.textContent = entry.value;
    row.prepend(name);
  }
}
