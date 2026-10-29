function make_withdraw(balance) {
    return amount => {
               if (balance >= amount) {
                   balance = balance - amount;
                   return balance;
               } else {
                   return "insufficient funds";
               }
           };
}
const W1 = make_withdraw(100);
W1(50);
const W2 = make_withdraw(100);
W2(80);

// expected: 20
