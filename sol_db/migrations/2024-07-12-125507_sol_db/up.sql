-- Your SQL goes here
CREATE TABLE `transactions`(
	`id` TEXT NOT NULL PRIMARY KEY,
	`confirmed` BOOL NOT NULL,
	`block_date` DATE NOT NULL,
	`block_ts` TIMESTAMP NOT NULL
);

CREATE TABLE `accounts`(
	`id` TEXT NOT NULL PRIMARY KEY,
	`lamports` TEXT NOT NULL,
	`owner` TEXT NOT NULL
);

