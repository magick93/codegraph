// Gate-owned stub: the consumer monorepo owns the real package.
// The scaffold's settings/auth/dashboard routes delegate to these
// components; stubs are runtime exports the bundler can resolve.
export interface CrewbaseComponentProps {
	[key: string]: unknown;
}
export function ApiKeySettings(_props: CrewbaseComponentProps) {}
export function TeamSettings(_props: CrewbaseComponentProps) {}
export function Dashboard(_props: CrewbaseComponentProps) {}
export function LoginPage(_props: CrewbaseComponentProps) {}
export function SignupPage(_props: CrewbaseComponentProps) {}
