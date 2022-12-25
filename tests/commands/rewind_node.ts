async function rewindNode() {
  const shell = require('shelljs');

  // --index
  // --to
  // --base-path
  // --backup-path
  if (process.argv.length < 2 || process.argv[2] === null || process.argv[2] === undefined) {
    console.error('please pass command options');
    return;
  }
  if (isNaN(Number(process.argv[2]))) {
    console.error('please pass a numeric node index');
    return;
  }
  if (isNaN(Number(process.argv[3]))) {
    console.error('please pass a numeric block number');
    return;
  }
  if (Number(process.argv[2]) < 1) {
    console.error('please pass a positive numeric node index');
    return;
  }
  if (Number(process.argv[3]) < 1) {
    console.error('please pass a positive numeric block number');
    return;
  }

  let index = Number(process.argv[2]);
  console.log(`[*] passed node index = ${index}`);

  let to = Number(process.argv[3]);
  console.log(`[*] passed block number = ${to}`);

  let basePath = `../data/boot${index}`;
  let backupPath = `../backup/boot${index}.blocks`;

  if (process.argv[4]) {
    basePath = process.argv[4];
  }
  if (process.argv[5]) {
    backupPath = process.argv[5];
  }
  console.log(`[*] node base path = ${basePath}`);
  console.log(`[*] node backup path = ${backupPath}`);

  shell.exec(`scripts/rewind_node.sh ${index} ${to} ${basePath} ${backupPath}`);
  console.log('[*] rewinded node');
}
rewindNode();
