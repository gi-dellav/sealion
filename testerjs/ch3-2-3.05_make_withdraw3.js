function make_withdraw(initial_amount) {
    return (balance => 
              amount => {
                  if (balance >= amount) {
                      balance = balance - amount;
                      return balance;
                   } else {
                      return "insufficient funds";
                   }
              })(initial_amount);
}

const W1 = make_withdraw(100);
W1(80);

// expected: 20
