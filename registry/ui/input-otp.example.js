// @flow
import { InputOtp, InputOtpGroup, InputOtpSeparator, InputOtpSlot } from "./input-otp.js";

/** A six-digit code in two groups of three. */
export component Example() {
  return (
    <InputOtp label="Verification code" length={6} name="code">
      <InputOtpGroup>
        <InputOtpSlot index={0} />
        <InputOtpSlot index={1} />
        <InputOtpSlot index={2} />
      </InputOtpGroup>
      <InputOtpSeparator />
      <InputOtpGroup>
        <InputOtpSlot index={3} />
        <InputOtpSlot index={4} />
        <InputOtpSlot index={5} />
      </InputOtpGroup>
    </InputOtp>
  );
}
