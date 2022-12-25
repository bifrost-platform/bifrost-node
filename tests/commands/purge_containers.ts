async function purgeContainers() {
  const shell = require('shelljs');

  shell.exec(`scripts/purge_docker_containers.sh`);
  console.log('[*] purged containers');
}

purgeContainers();
