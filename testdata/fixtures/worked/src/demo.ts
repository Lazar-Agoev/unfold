function save(value: number) {
  return value;
}

function run(values: number[]) {
  for (const value of values) {
    if (value > 0) save(value);
  }
}
