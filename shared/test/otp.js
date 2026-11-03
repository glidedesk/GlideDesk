// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// shared/otp.js: the one-time code in a notification (Copy on Overview and the toast), and only that.
'use strict';

const assert = require('assert');
const { otpOf } = require('../otp.js');

const cases = [
  ['Your one-time code is 482913. Don’t share it.', '482913'],
  ['G-482913 is your Google verification code.', '482913'],
  ['OTP: 4829-13 valid 5 min', '482913'],
  ['Verification code: 123 456', '123456'],
  ['Use 739201 to sign in to Instagram', '739201'],
  ['Your code 1234 for order 99887766', '1234'],
  ['Your total is 12.50, code 9921', '9921'],
  ['ඔබගේ කේතය 556677', '556677'],
  ['Order 99887766 shipped', ''],
  ['Meeting at 10:30, room 2024', ''],
  ['PIN 2024 expires', ''],
  ['Call me at 0771234567 about the code', ''],
  ['', ''],
];
for (const [text, want] of cases) assert.strictEqual(otpOf(text), want, JSON.stringify(text));
console.log('otp: ok');
