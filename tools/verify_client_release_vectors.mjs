// SPDX-License-Identifier: AGPL-3.0-or-later
// Independent profile/signature bytes only; this grants no installation authority.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { createHash, createPrivateKey, createPublicKey, sign, verify } from 'node:crypto';
const { Decoder, encode } = createRequire(new URL('../spikes/m00/web/package.json', import.meta.url))('cbor-x');
const fixture = JSON.parse(readFileSync(new URL('../protocol/fixtures/client-release-v1.json', import.meta.url), 'utf8'));
const bytes = value => Buffer.from(value, 'hex');
const decoder = new Decoder({ mapsAsObjects: false });
const encoded = bytes(fixture.cose_hex), publicBytes = bytes(fixture.public_hex);
const [protectedBytes, unprotected, payload, signature] = decoder.decode(encoded);
assert.equal(fixture.domain, 'bbsbox.client-release/v1');
assert.deepEqual(Buffer.from(protectedBytes), Buffer.concat([bytes('a20132045820'), publicBytes]));
assert.equal(unprotected.size, 0);
assert.deepEqual(Buffer.from(payload), bytes(fixture.payload_hex));
assert(Buffer.from(payload).toString('utf8').includes(`"sequence":${fixture.sequence},`));
assert.deepEqual(Buffer.from(encoded), Buffer.concat([bytes('84'), encode(protectedBytes), bytes('a0'), encode(payload), encode(signature)]));
assert.equal(createHash('sha256').update(encoded).digest('hex'), fixture.manifest_sha256);
assert.equal(fixture.key_id, `release-key:${createHash('sha256').update(Buffer.from('bbsbox.release-signing-key/v1\0')).update(publicBytes).digest('hex')}`);
const key = createPublicKey({ key: Buffer.concat([bytes('302a300506032b6570032100'), publicBytes]), format: 'der', type: 'spki' });
const message = (domain, body = payload) => Buffer.from(encode(['Signature1', protectedBytes, Buffer.from(domain), body]));
assert(verify(null, message(fixture.domain), key, signature));
assert(!verify(null, message('bbsbox.release-manifest/v1'), key, signature));
const changed = Buffer.from(payload); changed[0] ^= 1;
assert(!verify(null, message(fixture.domain, changed), key, signature));
const privateKey = createPrivateKey({ key: Buffer.concat([bytes('302e020100300506032b657004220420'), bytes(fixture.fixture_seed_hex)]), format: 'der', type: 'pkcs8' });
assert.deepEqual(sign(null, message(fixture.domain), privateKey), Buffer.from(signature));
process.stdout.write('PASS: independent client release bytes, integer boundary, key ID, domain and signature.\n');
