import { expect, test } from "vitest";
import { guessKey, parseHeader, sniffDelimiter } from "./csv";

test("quoted headers keep their commas and escaped quotes", () => {
  expect(parseHeader('id,"name, full","say ""hi""",x\n1,2,3,4', ",")).toEqual(["id", "name, full", 'say "hi"', "x"]);
});

test("a byte-order mark and CRLF don't leak into column names", () => {
  expect(parseHeader("﻿sku;price\r\nA;1", ";")).toEqual(["sku", "price"]);
});

test("the delimiter is the one that splits the header most", () => {
  expect(sniffDelimiter("a;b;c\n1;2;3")).toBe(";");
  expect(sniffDelimiter("a\tb\n")).toBe("\t");
  expect(sniffDelimiter("single\n")).toBe(",");
});

test("key guess prefers exact names, then *_id", () => {
  expect(guessKey(["name", "SKU", "price"])).toBe("SKU");
  expect(guessKey(["order_id", "total"])).toBe("order_id");
  expect(guessKey(["name", "price"])).toBeNull();
});
