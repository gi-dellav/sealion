let times = 10;
let counter = 2;

while (times > 0) {
	times -= 1;
	if (times % 2 === 0) {
		continue;
	}
	counter = counter * 2;
	if (counter === undefined) {
		break;
	}
}

counter;
