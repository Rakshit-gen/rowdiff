//! Write a pair of product-catalog exports to try rowdiff on.
//!
//!     cargo run --release --example gen -- 1000000 /tmp/before.csv /tmp/after.csv
//!
//! The second file drops about 0.5% of products, adds about 0.5% new ones,
//! changes price or stock on about 2%, and is written in a scrambled order so
//! the sort has real work to do. Same row count, same output, every run.

use std::fs::File;
use std::io::{BufWriter, Write};

/// xorshift64*, enough randomness for test data with no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const NAMES: [&str; 12] = [
    "Desk lamp",
    "Notebook, A5",
    "Steel bottle",
    "USB-C cable",
    "Wool socks",
    "Coffee grinder",
    "Phone stand",
    "Linen shirt",
    "Chef's knife",
    "Yoga mat",
    "Rain jacket",
    "Backpack, 20L",
];
const CATEGORIES: [&str; 5] = ["home", "office", "kitchen", "apparel", "outdoor"];

struct Product {
    sku: String,
    name: &'static str,
    category: &'static str,
    price_cents: u64,
    stock: u64,
}

fn product(i: u64) -> Product {
    let mut r = Rng(i.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    Product {
        sku: format!("SKU-{i:08}"),
        name: NAMES[r.below(NAMES.len() as u64) as usize],
        category: CATEGORIES[r.below(CATEGORIES.len() as u64) as usize],
        price_cents: 199 + r.below(20_000),
        stock: r.below(500),
    }
}

fn write(w: &mut impl Write, p: &Product) -> std::io::Result<()> {
    let name = if p.name.contains(',') {
        format!("\"{}\"", p.name)
    } else {
        p.name.to_string()
    };
    writeln!(
        w,
        "{},{},{},{}.{:02},{}",
        p.sku,
        name,
        p.category,
        p.price_cents / 100,
        p.price_cents % 100,
        p.stock
    )
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, n, a, b] = args.as_slice() else {
        eprintln!("usage: gen <rows> <before.csv> <after.csv>");
        std::process::exit(2);
    };
    let n: u64 = n.parse().expect("rows must be a number");
    let header = "sku,name,category,price,stock\n";

    let mut wa = BufWriter::new(File::create(a)?);
    wa.write_all(header.as_bytes())?;
    for i in 0..n {
        write(&mut wa, &product(i))?;
    }
    wa.flush()?;

    // Visit ids in a scrambled but complete order: i * step mod n hits every
    // id once when step and n share no factors.
    let mut step = n / 2 + 1;
    while gcd(step, n) != 1 {
        step += 1;
    }
    let mut wb = BufWriter::new(File::create(b)?);
    wb.write_all(header.as_bytes())?;
    let mut r = Rng(42);
    for j in 0..n {
        let i = (j as u128 * step as u128 % n as u128) as u64;
        let roll = r.below(1000);
        if roll < 5 {
            continue;
        }
        let mut p = product(i);
        if roll < 15 {
            p.price_cents = p.price_cents * (90 + r.below(25)) / 100;
        } else if roll < 25 {
            p.stock = r.below(500);
        }
        write(&mut wb, &p)?;
    }
    for i in n..n + n / 200 {
        write(&mut wb, &product(i))?;
    }
    wb.flush()
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}
