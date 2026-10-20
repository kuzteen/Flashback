export const ONBOARDING_STEPS = 5 as const;

export type OnboardingFlow = {
  step: number;
  skipDialog: boolean;
  skipped: boolean;
  completed: boolean;
};

export function initialOnboardingFlow(step = 0): OnboardingFlow {
  return { step: Math.max(0, Math.min(ONBOARDING_STEPS - 1, Math.trunc(step))), skipDialog: false, skipped: false, completed: false };
}

export function nextOnboardingStep(flow: OnboardingFlow): OnboardingFlow {
  return { ...flow, step: Math.min(ONBOARDING_STEPS - 1, flow.step + 1) };
}

export function previousOnboardingStep(flow: OnboardingFlow): OnboardingFlow {
  return { ...flow, step: Math.max(0, flow.step - 1) };
}

export function openSkipConfirmation(flow: OnboardingFlow): OnboardingFlow {
  return { ...flow, skipDialog: true };
}

export function cancelSkipConfirmation(flow: OnboardingFlow): OnboardingFlow {
  return { ...flow, skipDialog: false };
}

export function confirmSkip(flow: OnboardingFlow): OnboardingFlow {
  return { ...flow, skipDialog: false, skipped: true };
}

export function completeOnboarding(flow: OnboardingFlow): OnboardingFlow {
  return { ...flow, completed: true };
}
