// SPDX-License-Identifier: AGPL-3.0-or-later
// Independent release-manifest profile/signature check; no install/rollback authorization.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {createHash,createPrivateKey,createPublicKey,sign,verify} from 'node:crypto';
const {Decoder,encode}=createRequire(new URL('../spikes/m00/web/package.json',import.meta.url))('cbor-x');
const decoder=new Decoder({mapsAsObjects:false});
const fixture=JSON.parse(readFileSync(new URL('../protocol/fixtures/release-manifest-v1.json',import.meta.url),'utf8'));
const b=value=>Buffer.from(value,'hex');
const bytes=b(fixture.cose_hex),publicBytes=b(fixture.public_hex);
const [protectedBytes,unprotected,payload,signature]=decoder.decode(bytes);
assert.equal(fixture.domain,'bbsbox.release-manifest/v1');
assert.equal(decoder.decode(protectedBytes).get(1),-19);
assert.deepEqual(Buffer.from(protectedBytes),Buffer.concat([Buffer.from([0xa2,1,0x32,4,0x58,0x20]),publicBytes]));
assert.equal(unprotected.size,0);assert.deepEqual(Buffer.from(payload),b(fixture.payload_hex));
const text=Buffer.from(payload).toString('utf8');
// Keep the signed integers as bytes rather than rounding/re-encoding through JavaScript numbers.
assert(text.includes(`"sequence":${fixture.sequence},`));assert(text.includes(`"created_at":${fixture.created_at},`));
assert.deepEqual(Buffer.from(bytes),Buffer.concat([Buffer.from([0x84]),Buffer.from(encode(protectedBytes)),Buffer.from([0xa0]),Buffer.from(encode(payload)),Buffer.from(encode(signature))]));
assert.equal(createHash('sha256').update(bytes).digest('hex'),fixture.manifest_sha256);
assert.equal(fixture.key_id,`release-key:${createHash('sha256').update(Buffer.from('bbsbox.release-signing-key/v1\0')).update(publicBytes).digest('hex')}`);
const key=createPublicKey({key:Buffer.concat([b('302a300506032b6570032100'),publicBytes]),format:'der',type:'spki'});
const tbs=domain=>Buffer.from(encode(['Signature1',protectedBytes,Buffer.from(domain),payload]));
assert(verify(null,tbs(fixture.domain),key,signature));
assert(!verify(null,tbs('bbsbox.archive-inventory/v1'),key,signature));
const privateKey=createPrivateKey({key:Buffer.concat([b('302e020100300506032b657004220420'),b(fixture.fixture_seed_hex)]),format:'der',type:'pkcs8'});
assert.deepEqual(sign(null,tbs(fixture.domain),privateKey),Buffer.from(signature));
process.stdout.write('PASS: independent release manifest bytes, key ID, integer boundaries, domain and Ed25519 signature.\n');
