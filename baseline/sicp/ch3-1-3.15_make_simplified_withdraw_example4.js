function make_simplified_withdraw(balance) {
    return amount => {
               balance = balance - amount;
               return balance;
           };
}
const W1 = make_simplified_withdraw(25);

const W2 = make_simplified_withdraw(25);
W1(20);

// expected: 5
