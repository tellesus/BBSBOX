// SPDX-License-Identifier: AGPL-3.0-or-later
// Independent CBOR/Node crypto checks for current issuance and immutable legacy verification.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {createHash,createPrivateKey,createPublicKey,sign,verify} from 'node:crypto';
const {Decoder,encode}=createRequire(new URL('../spikes/m00/web/package.json',import.meta.url))('cbor-x');
const decoder=new Decoder({mapsAsObjects:false});
const fixture=name=>JSON.parse(readFileSync(new URL(`../crates/credentials/tests/fixtures/${name}`,import.meta.url),'utf8'));
function cose(vector,domain,algorithm,personDigest,payloadExpected) {
  const bytes=Buffer.from(vector.cose_hex,'hex');
  const [protectedBytes,unprotected,payload,signature]=decoder.decode(bytes);
  assert.equal(unprotected.size,0);
  const headers=decoder.decode(protectedBytes);assert.equal(headers.size,2);assert.equal(headers.get(1),algorithm);
  assert.deepEqual(Buffer.from(headers.get(4)),personDigest);
  assert.deepEqual(Buffer.from(payload),Buffer.from(encode(payloadExpected)));
  const root=Buffer.from(vector.root_public_hex,'hex');
  const input=Buffer.from(encode(['Signature1',protectedBytes,Buffer.from(domain),payload]));
  const key=createPublicKey({key:Buffer.concat([Buffer.from('302a300506032b6570032100','hex'),root]),format:'der',type:'spki'});
  assert(verify(null,input,key,signature));
  const privateKey=createPrivateKey({key:Buffer.concat([Buffer.from('302e020100300506032b657004220420','hex'),Buffer.from(vector.fixture_root_seed_hex,'hex')]),format:'der',type:'pkcs8'});
  assert.deepEqual(sign(null,input,privateKey),Buffer.from(signature));
  const corrupt=Buffer.from(input);corrupt[corrupt.length-1]^=1;assert(!verify(null,corrupt,key,signature));
  return {bytes,protectedBytes,payload,signature,key};
}
for (const version of [1,2]) {
  const vector=fixture(`device-v${version}.json`);const algorithm=version===1?-8:-19;
  assert.equal(vector.format,`bbsbox.device-credential/v${version}`);
  const root=Buffer.from(vector.root_public_hex,'hex');const device=Buffer.from(vector.device_public_hex,'hex');
  const personDigest=createHash('sha256').update(Buffer.from('bbsbox.person/v1\0')).update(root).digest();
  assert.equal(vector.person_id,`person:${personDigest.toString('hex')}`);
  assert.equal(vector.device_id,`device:${createHash('sha256').update(Buffer.from('bbsbox.device/v1\0')).update(device).digest('hex')}`);
  const deviceCose=cose(vector,vector.format,algorithm,personDigest,[vector.format,version,root,device,Buffer.from(vector.incarnation_hex,'hex'),9]);
  assert.equal(vector.credential_id,`credential:${createHash('sha256').update(deviceCose.bytes).digest('hex')}`);
  const link=fixture(`link-v${version}.json`);assert.equal(link.format,`bbsbox.account-link/v${version}`);
  const nodeDigest=createHash('sha256').update(Buffer.from('bbsbox.node/v1\0')).update(Buffer.from(link.node_public_hex,'hex')).digest();
  assert.equal(link.node_id,`node:${nodeDigest.toString('hex')}`);assert.equal(link.person_id,vector.person_id);assert.equal(link.root_public_hex,vector.root_public_hex);
  const linkBody=[link.format,version,nodeDigest,Buffer.from(link.account_hex,'hex'),personDigest,Buffer.from(link.nonce_hex,'hex')];
  const linkCose=cose(link,link.format,algorithm,personDigest,linkBody);
  const wrongAccount=[...linkBody];wrongAccount[3]=Buffer.alloc(16,99);
  assert(!verify(null,Buffer.from(encode(['Signature1',linkCose.protectedBytes,Buffer.from(link.format),encode(wrongAccount)])),linkCose.key,linkCose.signature));
}
process.stdout.write('PASS: independent current-v2 and legacy-v1 device/link proof vectors.\n');
