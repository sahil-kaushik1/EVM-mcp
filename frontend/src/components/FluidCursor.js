import React, { useEffect } from 'react';
import fluidCursor from '../hooks/use-FluidCursor';

const FluidCursor = () => {
  useEffect(() => {
    // Add a small delay to ensure the canvas is properly rendered
    const timer = setTimeout(() => {
      const canvas = document.getElementById('fluid');
      if (!canvas) {
        console.warn('FluidCursor: Canvas element not found');
        return;
      }

      try {
        fluidCursor();
      } catch (error) {
        console.error('FluidCursor initialization error:', error);
      }
    }, 100); // 100ms delay

    return () => clearTimeout(timer);
  }, []);

  return (
    <div className="fluid-container">
      <canvas id="fluid" className="fluid-canvas" />
    </div>
  );
};

export default FluidCursor;