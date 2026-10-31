// SPDX-License-Identifier: AGPL-3.0-or-later
async function loadStatus(){
  try{
    const response=await fetch('/api/v1/node',{cache:'no-store',credentials:'same-origin'});
    if(!response.ok)throw new Error('status unavailable');
    const node=await response.json();
    document.querySelector('#name').textContent=node.name;
    document.querySelector('#node').textContent=node.node_id;
    document.querySelector('#version').textContent=node.version;
    const status=document.querySelector('#state');status.textContent='Node ready';status.classList.add('ready');
    document.querySelector('#detail').textContent='Local service is running.';
  }catch{
    document.querySelector('#state').textContent='Status unavailable';
    document.querySelector('#detail').textContent='Reload this page to check the node again.';
  }
}
loadStatus();
