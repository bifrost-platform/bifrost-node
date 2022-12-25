/**
 * purge chain data
 */
async function purgeChains() {
  const shell = require('shelljs');

  // purge_chains.sh
  shell.exec(`scripts/purge_chains.sh`);
  console.log('[*] purged chains');
}
purgeChains();
