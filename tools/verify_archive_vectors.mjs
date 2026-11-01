// SPDX-License-Identifier: AGPL-3.0-or-later
// Independent verification of the maintenance inventory, distinct from signed events.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {createHash,createPrivateKey,createPublicKey,sign,verify} from 'node:crypto';
const {Decoder,encode}=createRequire(new URL('../spikes/m00/web/package.json',import.meta.url))('cbor-x');
const decoder=new Decoder({mapsAsObjects:false});
const fixture=JSON.parse(readFileSync(new URL('../protocol/fixtures/archive-inventory-v1.json',import.meta.url),'utf8'));
const b=value=>Buffer.from(value,'hex');
const bytes=b(fixture.cose_hex),publicBytes=b(fixture.public_hex);
const [protectedBytes,unprotected,payload,signature]=decoder.decode(bytes);
assert.equal(fixture.domain,'bbsbox.archive-inventory/v1');
assert.equal(decoder.decode(protectedBytes).get(1),-19);
assert.deepEqual(Buffer.from(protectedBytes),Buffer.concat([Buffer.from([0xa2,1,0x32,4,0x58,0x20]),publicBytes]));
assert.equal(unprotected.size,0);
assert.deepEqual(Buffer.from(payload),b(fixture.payload_hex));
// Preserve the exact JSON bytes: its signed i64 timestamp exceeds JavaScript's safe integers.
assert(Buffer.from(payload).toString('utf8').includes('"created_at":-9223372036854775808,'));
assert.deepEqual(Buffer.from(bytes),Buffer.concat([Buffer.from([0x84]),Buffer.from(encode(protectedBytes)),Buffer.from([0xa0]),Buffer.from(encode(payload)),Buffer.from(encode(signature))]));
assert.equal(createHash('sha256').update(bytes).digest('hex'),fixture.inventory_sha256);
assert.equal(fixture.node,`node:${createHash('sha256').update(Buffer.from('bbsbox.node/v1\0')).update(publicBytes).digest('hex')}`);
const key=createPublicKey({key:Buffer.concat([b('302a300506032b6570032100'),publicBytes]),format:'der',type:'spki'});
const tbs=domain=>Buffer.from(encode(['Signature1',protectedBytes,Buffer.from(domain),payload]));
assert(verify(null,tbs(fixture.domain),key,signature));
assert(!verify(null,tbs('bbsbox.event/v1'),key,signature));
const tampered=Buffer.from(signature);tampered[0]^=1;
assert(!verify(null,tbs(fixture.domain),key,tampered));
const privateKey=createPrivateKey({key:Buffer.concat([b('302e020100300506032b657004220420'),b(fixture.fixture_seed_hex)]),format:'der',type:'pkcs8'});
assert.deepEqual(sign(null,tbs(fixture.domain),privateKey),Buffer.from(signature));
process.stdout.write('PASS: independent archive inventory bytes, node identity, hash, domain and Ed25519 signature.\n');
