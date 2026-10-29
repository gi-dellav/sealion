function factorial(n) {
    let product = 1;
    let counter = 1;
    while_loop(() => counter <= n,
               () => {
                   product = counter * product;
                   counter = counter + 1;
               });
    return product;
}
