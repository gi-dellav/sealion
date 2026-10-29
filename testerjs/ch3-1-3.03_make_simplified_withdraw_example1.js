function make_simplified_withdraw(balance) {
    return amount => {
               balance = balance - amount;
               return balance;
           };
}
const W = make_simplified_withdraw(25);
W(20);

// expected: 5
