// SPDX-License-Identifier: AGPL-3.0-or-later
// Independent CBOR/Node crypto conformance check of the public Rust-produced vector.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {createHash,createPrivateKey,createPublicKey,sign,verify} from 'node:crypto';
const {Decoder,encode}=createRequire(new URL('../spikes/m00/web/package.json',import.meta.url))('cbor-x');
const decoder=new Decoder({mapsAsObjects:false});
const vector=JSON.parse(readFileSync(new URL('../crates/credentials/tests/fixtures/device-v1.json',import.meta.url),'utf8'));
const bytes=Buffer.from(vector.cose_hex,'hex');
const [protectedBytes,unprotected,payload,signature]=decoder.decode(bytes);
assert.equal(unprotected.size,0);
const headers=decoder.decode(protectedBytes);
assert.equal(headers.size,2);assert.equal(headers.get(1),-8);
const root=Buffer.from(vector.root_public_hex,'hex');const device=Buffer.from(vector.device_public_hex,'hex');
const expectedBody=['bbsbox.device-credential/v1',1,root,device,Buffer.from(vector.incarnation_hex,'hex'),9];
assert.deepEqual(Buffer.from(payload),Buffer.from(encode(expectedBody)));
const personDigest=createHash('sha256').update(Buffer.from('bbsbox.person/v1\0')).update(root).digest();
assert.equal(vector.person_id,`person:${personDigest.toString('hex')}`);assert.deepEqual(Buffer.from(headers.get(4)),personDigest);
assert.equal(vector.device_id,`device:${createHash('sha256').update(Buffer.from('bbsbox.device/v1\0')).update(device).digest('hex')}`);
assert.equal(vector.credential_id,`credential:${createHash('sha256').update(bytes).digest('hex')}`);
const input=Buffer.from(encode(['Signature1',protectedBytes,Buffer.from('bbsbox.device-credential/v1'),payload]));
const key=createPublicKey({key:Buffer.concat([Buffer.from('302a300506032b6570032100','hex'),root]),format:'der',type:'spki'});
assert(verify(null,input,key,signature));
const privateKey=createPrivateKey({key:Buffer.concat([Buffer.from('302e020100300506032b657004220420','hex'),Buffer.from(vector.fixture_root_seed_hex,'hex')]),format:'der',type:'pkcs8'});
assert.deepEqual(sign(null,input,privateKey),Buffer.from(signature));
const corrupt=Buffer.from(input);corrupt[corrupt.length-1]^=1;assert(!verify(null,corrupt,key,signature));
process.stdout.write('PASS: independent CBOR, identity hashes and Ed25519 signing/verification vector.\n');
