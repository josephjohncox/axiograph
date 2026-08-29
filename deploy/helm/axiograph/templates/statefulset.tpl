apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: {{ include "axiograph.fullname" . }}-db
  labels:
    {{- include "axiograph.labels" . | nindent 4 }}
spec:
  serviceName: {{ include "axiograph.fullname" . }}-db-headless
  replicas: {{ .Values.statefulset.replicas }}
  selector:
    matchLabels:
      app.kubernetes.io/name: {{ include "axiograph.name" . }}
      app.kubernetes.io/instance: {{ .Release.Name }}
  template:
    metadata:
      labels:
        app.kubernetes.io/name: {{ include "axiograph.name" . }}
        app.kubernetes.io/instance: {{ .Release.Name }}
    spec:
      securityContext:
        runAsNonRoot: true
        runAsUser: 10001
        runAsGroup: 10001
        fsGroup: 10001
        seccompProfile:
          type: RuntimeDefault
      volumes:
        - name: tmp
          emptyDir:
            sizeLimit: 64Mi
      {{- if not .Values.statefulset.persistence.enabled }}
        - name: data
          emptyDir: {}
      {{- end }}
      {{- $imageTag := .Values.image.tag | default (printf "v%s" .Chart.AppVersion) }}
      containers:
        - name: axiograph
          image: "{{ .Values.image.repository }}:{{ $imageTag }}"
          imagePullPolicy: {{ .Values.image.pullPolicy }}
          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            capabilities:
              drop:
                - ALL
          args:
            {{- toYaml .Values.statefulset.args | nindent 12 }}
          {{- if .Values.statefulset.env }}
          env:
            {{- toYaml .Values.statefulset.env | nindent 12 }}
          {{- end }}
          ports:
            - name: http
              containerPort: {{ .Values.service.port }}
          readinessProbe:
            httpGet:
              path: /status
              port: http
            initialDelaySeconds: 5
            periodSeconds: 10
          livenessProbe:
            httpGet:
              path: /status
              port: http
            initialDelaySeconds: 10
            periodSeconds: 20
          volumeMounts:
            - name: data
              mountPath: /data
            - name: tmp
              mountPath: /tmp
          resources:
            {{- toYaml .Values.statefulset.resources | nindent 12 }}
  {{- if .Values.statefulset.persistence.enabled }}
  volumeClaimTemplates:
    - metadata:
        name: data
      spec:
        accessModes: ["ReadWriteOnce"]
        {{- if .Values.statefulset.persistence.storageClass }}
        storageClassName: {{ .Values.statefulset.persistence.storageClass | quote }}
        {{- end }}
        resources:
          requests:
            storage: {{ .Values.statefulset.persistence.size }}
  {{- end }}
