// @flow
import * as InputOtp from "./input-otp.js";

/** A six-digit code in two groups of three. */
export component Example() {
  return (
    <InputOtp.Root label="Verification code" length={6} name="code">
      <InputOtp.Group>
        <InputOtp.Slot index={0} />
        <InputOtp.Slot index={1} />
        <InputOtp.Slot index={2} />
      </InputOtp.Group>
      <InputOtp.Separator />
      <InputOtp.Group>
        <InputOtp.Slot index={3} />
        <InputOtp.Slot index={4} />
        <InputOtp.Slot index={5} />
      </InputOtp.Group>
    </InputOtp.Root>
  );
}
