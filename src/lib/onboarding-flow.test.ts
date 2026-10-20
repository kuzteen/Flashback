import { describe, expect, it } from 'vitest';
import {
  cancelSkipConfirmation,
  completeOnboarding,
  confirmSkip,
  initialOnboardingFlow,
  nextOnboardingStep,
  openSkipConfirmation,
  previousOnboardingStep
} from './onboarding-flow';

describe('onboarding flow', () => {
  it('starts on the first of exactly five steps and clamps saved progress', () => {
    expect(initialOnboardingFlow()).toMatchObject({ step: 0 });
    expect(initialOnboardingFlow(3).step).toBe(3);
    expect(initialOnboardingFlow(99).step).toBe(4);
  });

  it('moves back and forward without leaving the five steps', () => {
    expect(previousOnboardingStep(initialOnboardingFlow()).step).toBe(0);
    expect(nextOnboardingStep(initialOnboardingFlow(4)).step).toBe(4);
    expect(nextOnboardingStep(initialOnboardingFlow(2)).step).toBe(3);
  });

  it('cancelling skip closes the dialog and preserves progress', () => {
    const current = openSkipConfirmation(initialOnboardingFlow(2));
    expect(cancelSkipConfirmation(current)).toEqual({
      step: 2,
      skipDialog: false,
      skipped: false,
      completed: false
    });
  });

  it('only confirmed skip records a skip decision', () => {
    expect(confirmSkip(openSkipConfirmation(initialOnboardingFlow(1)))).toMatchObject({
      step: 1,
      skipDialog: false,
      skipped: true,
      completed: false
    });
  });

  it('normal completion is separate from skip confirmation', () => {
    expect(completeOnboarding(initialOnboardingFlow(4))).toMatchObject({
      step: 4,
      skipDialog: false,
      skipped: false,
      completed: true
    });
  });
});
