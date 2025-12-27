{{- define "axiograph.name" -}}
axiograph
{{- end -}}

{{- define "axiograph.fullname" -}}
{{- printf "%s-%s" .Release.Name (include "axiograph.name" .) | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "axiograph.labels" -}}
app.kubernetes.io/name: {{ include "axiograph.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end -}}
