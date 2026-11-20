function make_withdraw(balance) {
    return amount => {
               if (balance >= amount) {
                   balance = balance - amount;
                   return balance;
               } else {
                   return "Insufficient funds";
               }
           };
}

const W1 = make_withdraw(100);
const W2 = make_withdraw(100);
