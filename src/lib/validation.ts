// Mirrors the backend's validation.validate_password, so the form can say what
// is wrong before submitting.
export function validatePassword(password: string): string | null {
  if (password.length < 8) {
    return 'Password must be at least 8 characters long'
  }
  if (!/[A-Za-z]/.test(password)) {
    return 'Password must contain at least 1 letter'
  }
  if (!/\d/.test(password)) {
    return 'Password must contain at least 1 digit'
  }
  return null
}
