// @vitest-environment jsdom
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, expect, test, vi } from "vitest";
import type { Change, Status } from "./api";
import { ChangeTable } from "./ChangeTable";

const rows: Change[] = Array.from({ length: 5 }, (_, i) => ({
  kind: "added",
  key: [`k${i}`],
  row: { id: `k${i}`, v: String(i) },
}));

vi.mock("./api", () => ({
  getRows: (_id: number, _f: unknown, offset: number, limit: number) =>
    Promise.resolve({ total: rows.length, offset, rows: rows.slice(offset, offset + limit) }),
}));

beforeAll(() => {
  // jsdom has neither of these.
  globalThis.ResizeObserver = class {
    observe() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
    // Like browsers, move focus to the first control inside.
    this.querySelector<HTMLElement>("button:not(:disabled)")?.focus();
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
    this.dispatchEvent(new Event("close"));
  };
});

const status = {
  id: 1,
  key: ["id"],
  a: { name: "a.csv", columns: ["id", "v"] },
  b: { name: "b.csv", columns: ["id", "v"] },
  compared_columns: ["v"],
} as Status;

test("arrow keys move between rows, Enter opens one, arrows step inside", async () => {
  const user = userEvent.setup();
  render(<ChangeTable status={status} filter={{ kind: "added" }} total={rows.length} />);
  await screen.findByText("k0");

  await act(() => screen.getByText("k0").closest<HTMLElement>("[role=row]")!.focus());
  await user.keyboard("{ArrowDown}j");
  expect(document.activeElement?.getAttribute("data-i")).toBe("2");

  await user.keyboard("{Enter}");
  const dialog = await screen.findByRole("dialog");
  expect(dialog.textContent).toContain("k2");
  expect(dialog.textContent).toContain("3 of 5");

  await user.keyboard("{ArrowRight}");
  await waitFor(() => expect(dialog.textContent).toContain("4 of 5"));
  expect(dialog.textContent).toContain("k3");
});
