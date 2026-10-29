let balance = 100;

function withdraw(amount) {
    if (balance >= amount) {
        balance = balance - amount;
        return balance;
    } else {
        return "Insufficient funds";
    }
}
withdraw(25);
withdraw(25);
withdraw(60);
withdraw(15);

// expected: 35
