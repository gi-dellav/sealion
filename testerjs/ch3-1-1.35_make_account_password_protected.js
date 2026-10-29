function make_account(balance, p) {
    function withdraw(amount) {
        if (balance >= amount) {
            balance = balance - amount;
            return balance;
        } else {
            return "Insufficient funds";
        }
    }
    function deposit(amount) {
        balance = balance + amount;
        return balance;
    }
    function dispatch(m, q) {
        if (p === q) {
            return m === "withdraw"
                   ? withdraw
                   : m === "deposit"
                   ? deposit
                   : "Unknown request: make_account";
        } else {
            return q => "Incorrect Password";
        }
    }
    return dispatch;
}

const a = make_account(100, "eva");
a("withdraw", "eva")(50); //withdraws 50
a("withdraw", "ben")(40); //incorrect password

// expected: 'Incorrect Password'
