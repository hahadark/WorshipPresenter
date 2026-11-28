module.exports = async (page, c) => {
  await c.click(150, 383); await c.sleep(500);
  const a = await c.shot('item7');
  await c.click(150, 383, 2); await c.sleep(500);
  const b = await c.shot('item7-dbl');
  return { a, b };
};
