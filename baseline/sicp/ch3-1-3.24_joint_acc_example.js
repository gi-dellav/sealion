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
const peter_acc = make_account(100, "open sesame");
peter_acc("withdraw", "open sesame")(10); // Withdraws 10
peter_acc("withdraw", "ben")(40); // Incorrect Password

// Make Joint Account
const paul_acc = make_joint(peter_acc, "open sesame", "rosebud");

paul_acc("withdraw", "rosebud")(50); // Withdraws 50, should return 40
