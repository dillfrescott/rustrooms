

        (function() {
            const canvas = document.getElementById('particleCanvas');
            const ctx = canvas.getContext('2d');
            const overlay = document.getElementById('welcomeOverlay');
            let particles = [];
            let animationId = null;

            function resize() {
                canvas.width = window.innerWidth;
                canvas.height = window.innerHeight;
            }
            resize();
            window.addEventListener('resize', resize);

            class Particle {
                constructor() {
                    this.x = Math.random() * canvas.width;
                    this.y = Math.random() * canvas.height;
                    this.vx = (Math.random() - 0.5) * 0.5;
                    this.vy = (Math.random() - 0.5) * 0.5;
                    this.radius = Math.random() * 2 + 1;
                    this.opacity = Math.random() * 0.5 + 0.2;
                    this.color = Math.random() > 0.5 ? '129, 140, 248' : '99, 102, 241';
                }
                update() {
                    this.x += this.vx;
                    this.y += this.vy;
                    if (this.x < 0) this.x = canvas.width;
                    if (this.x > canvas.width) this.x = 0;
                    if (this.y < 0) this.y = canvas.height;
                    if (this.y > canvas.height) this.y = 0;
                }
                draw() {
                    ctx.beginPath();
                    ctx.arc(this.x, this.y, this.radius, 0, Math.PI * 2);
                    ctx.fillStyle = `rgba(${this.color}, ${this.opacity})`;
                    ctx.fill();
                }
            }

            function init() {
                particles = [];
                const particleCount = Math.floor((canvas.width * canvas.height) / 18000);
                for (let i = 0; i < particleCount; i++) {
                    particles.push(new Particle());
                }
            }

            function animate() {
                if (particles.length === 0) init();

                ctx.clearRect(0, 0, canvas.width, canvas.height);
                particles.forEach(p => {
                    p.update();
                    p.draw();
                });
                animationId = requestAnimationFrame(animate);
            }

            function checkVisibility() {
                const style = window.getComputedStyle(overlay);
                const isVisible = style.display !== 'none' && 
                                  style.visibility !== 'hidden' && 
                                  !document.hidden;
                if (isVisible) {
                    if (!animationId) {
                        animate();
                    }
                } else {
                    if (animationId) {
                        cancelAnimationFrame(animationId);
                        animationId = null;
                    }
                    particles = [];
                    ctx.clearRect(0, 0, canvas.width, canvas.height);
                }
            }

            if (overlay) {
                checkVisibility();

                const observer = new MutationObserver(() => {
                    checkVisibility();
                });
                observer.observe(overlay, { attributes: true, attributeFilter: ['style', 'class'] });

                document.addEventListener('visibilitychange', checkVisibility);
            }
        })();

        (function() {
            const canvas = document.getElementById('particleCanvasConfig');
            const ctx = canvas.getContext('2d');
            const overlay = document.getElementById('configOverlay');
            let particles = [];
            let animationId = null;

            function resize() {
                canvas.width = window.innerWidth;
                canvas.height = window.innerHeight;
            }
            resize();
            window.addEventListener('resize', resize);

            class Particle {
                constructor() {
                    this.x = Math.random() * canvas.width;
                    this.y = Math.random() * canvas.height;
                    this.vx = (Math.random() - 0.5) * 0.5;
                    this.vy = (Math.random() - 0.5) * 0.5;
                    this.radius = Math.random() * 2 + 1;
                    this.opacity = Math.random() * 0.5 + 0.2;
                    this.color = Math.random() > 0.5 ? '129, 140, 248' : '99, 102, 241';
                }
                update() {
                    this.x += this.vx;
                    this.y += this.vy;
                    if (this.x < 0) this.x = canvas.width;
                    if (this.x > canvas.width) this.x = 0;
                    if (this.y < 0) this.y = canvas.height;
                    if (this.y > canvas.height) this.y = 0;
                }
                draw() {
                    ctx.beginPath();
                    ctx.arc(this.x, this.y, this.radius, 0, Math.PI * 2);
                    ctx.fillStyle = `rgba(${this.color}, ${this.opacity})`;
                    ctx.fill();
                }
            }

            function init() {
                particles = [];
                const particleCount = Math.floor((canvas.width * canvas.height) / 18000);
                for (let i = 0; i < particleCount; i++) {
                    particles.push(new Particle());
                }
            }

            function animate() {
                if (particles.length === 0) init();

                ctx.clearRect(0, 0, canvas.width, canvas.height);
                particles.forEach(p => {
                    p.update();
                    p.draw();
                });
                animationId = requestAnimationFrame(animate);
            }

            function checkVisibility() {
                const style = window.getComputedStyle(overlay);
                const isVisible = style.display !== 'none' && 
                                  style.visibility !== 'hidden' && 
                                  !document.hidden;
                if (isVisible) {
                    if (!animationId) {
                        animate();
                    }
                } else {
                    if (animationId) {
                        cancelAnimationFrame(animationId);
                        animationId = null;
                    }
                    particles = [];
                    ctx.clearRect(0, 0, canvas.width, canvas.height);
                }
            }

            if (overlay) {
                checkVisibility();

                const observer = new MutationObserver(() => {
                    checkVisibility();
                });
                observer.observe(overlay, { attributes: true, attributeFilter: ['style', 'class'] });

                document.addEventListener('visibilitychange', checkVisibility);
            }
        })();

        // Particle background for invite welcome overlay
        (function() {
            const canvas = document.getElementById('particleCanvasInvite');
            if (!canvas) return;
            const ctx = canvas.getContext('2d');
            const overlay = document.getElementById('inviteWelcomeOverlay');
            let particles = [];
            let animationId = null;

            function resize() {
                canvas.width = window.innerWidth;
                canvas.height = window.innerHeight;
            }
            resize();
            window.addEventListener('resize', resize);

            class Particle {
                constructor() {
                    this.x = Math.random() * canvas.width;
                    this.y = Math.random() * canvas.height;
                    this.vx = (Math.random() - 0.5) * 0.5;
                    this.vy = (Math.random() - 0.5) * 0.5;
                    this.radius = Math.random() * 2 + 1;
                    this.opacity = Math.random() * 0.5 + 0.2;
                    this.color = Math.random() > 0.5 ? '129, 140, 248' : '99, 102, 241';
                }
                update() {
                    this.x += this.vx;
                    this.y += this.vy;
                    if (this.x < 0) this.x = canvas.width;
                    if (this.x > canvas.width) this.x = 0;
                    if (this.y < 0) this.y = canvas.height;
                    if (this.y > canvas.height) this.y = 0;
                }
                draw() {
                    ctx.beginPath();
                    ctx.arc(this.x, this.y, this.radius, 0, Math.PI * 2);
                    ctx.fillStyle = `rgba(${this.color}, ${this.opacity})`;
                    ctx.fill();
                }
            }

            function init() {
                particles = [];
                const particleCount = Math.floor((canvas.width * canvas.height) / 18000);
                for (let i = 0; i < particleCount; i++) {
                    particles.push(new Particle());
                }
            }

            function animate() {
                if (particles.length === 0) init();

                ctx.clearRect(0, 0, canvas.width, canvas.height);
                particles.forEach(p => {
                    p.update();
                    p.draw();
                });
                animationId = requestAnimationFrame(animate);
            }

            function checkVisibility() {
                const style = window.getComputedStyle(overlay);
                const isVisible = style.display !== 'none' && 
                                  style.visibility !== 'hidden' && 
                                  !document.hidden;
                if (isVisible) {
                    if (!animationId) {
                        animate();
                    }
                } else {
                    if (animationId) {
                        cancelAnimationFrame(animationId);
                        animationId = null;
                    }
                    particles = [];
                    ctx.clearRect(0, 0, canvas.width, canvas.height);
                }
            }

            if (overlay) {
                checkVisibility();

                const observer = new MutationObserver(() => {
                    checkVisibility();
                });
                observer.observe(overlay, { attributes: true, attributeFilter: ['style', 'class'] });

                document.addEventListener('visibilitychange', checkVisibility);
            }
        })();
