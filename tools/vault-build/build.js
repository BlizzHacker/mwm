const fs = require("node:fs");
const path = require("node:path");
const esbuild = require("esbuild");
const root = path.resolve(__dirname, "../..");
const copies = [
  ["kdbxweb/LICENSE", "app/ui/vendor/kdbxweb.LICENSE"],
  ["hash-wasm/dist/argon2.umd.min.js", "app/ui/vendor/argon2.umd.min.js"],
  ["hash-wasm/LICENSE", "app/ui/vendor/hash-wasm.LICENSE"],
  ["@xmldom/xmldom/LICENSE", "app/ui/vendor/xmldom.LICENSE"],
];
for (const [source, dest] of copies) {
  const out = path.join(root, dest);
  fs.mkdirSync(path.dirname(out), { recursive: true });
  fs.copyFileSync(path.join(__dirname, "node_modules", source), out);
}
esbuild.buildSync({
  entryPoints: [path.join(__dirname, "node_modules/kdbxweb/lib/index.ts")],
  bundle: true, minify: true, platform: "browser", format: "iife", globalName: "kdbxweb",
  target: "es2020", define: { global: "globalThis" },
  alias: { crypto: path.join(__dirname, "browser-crypto-stub.js") },
  outfile: path.join(root, "app/ui/vendor/kdbxweb.min.js"),
  banner: { js: "/*! KdbxWeb 2.1.1 (MIT); bundled with @xmldom/xmldom 0.8.15 (MIT). See accompanying licenses. */" }
});
