// See "Node.js ES module support" at https://docs.rs/getrandom/0.2.8/getrandom/
const { webcrypto } = require('node:crypto')
globalThis.crypto = webcrypto

const { env } = require('node:process');
env.GITHUB_RUST_ACTION_PHASE = 'main';
const impl = require('./index.js');
